use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};
use std::time::{Duration, Instant};

use super::causal::{CycleKind, RuleCycle, RuleStep, Trace};
use crate::simulation::{
    CheckDefinition, Evaluation, Predicate, Quantity, QuantityBasis, RuleOccurrence,
};

#[derive(Debug, Clone)]
pub struct ResolvedCheck {
    pub definition: CheckDefinition,
    pub record: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Limit {
    Evaluations,
    WallTime,
    Cancelled,
    Restriction(String),
    Evidence,
}

#[derive(Debug, Clone)]
pub struct Control(Arc<Mutex<State>>);

#[derive(Debug)]
struct State {
    evaluation_limit: u64,
    wall_time: Option<Duration>,
    replay_checkpoint: Option<u64>,
    replay_limit: Limit,
    checkpoints: u64,
    evaluations: u64,
    active_since: Option<Instant>,
    paused: bool,
    cancel_requested: bool,
    waiters: Vec<Waker>,
    elapsed: Duration,
    stopped: Option<Limit>,
    now_ms: i64,
    trace: Trace,
    trace_bytes: u64,
    checks: Vec<ResolvedCheck>,
}

impl Control {
    pub fn record_transfer_change(&self, cell: &str, occurrence: &crate::simulation::RuleOccurrence, change: super::causal::TransferChange) -> Result<(), Limit> {
        let mut state = self.0.lock().expect("execution control");
        let limit = state.trace_bytes;
        if state.trace.record_transfer_change(cell, occurrence, change, limit).is_err() { state.stopped.get_or_insert(Limit::Evidence); }
        state.stopped.clone().map_or(Ok(()), Err)
    }

    pub fn record_received_transfer_change(&self, command: &str, change: super::causal::TransferChange) -> Result<(), Limit> {
        let mut state = self.0.lock().expect("execution control");
        let limit = state.trace_bytes;
        if state.trace.record_received_transfer_change(command, change, limit).is_err() { state.stopped.get_or_insert(Limit::Evidence); }
        state.stopped.clone().map_or(Ok(()), Err)
    }
    pub fn new(
        evaluation_limit: u64,
        wall_time_ms: Option<u64>,
        replay_checkpoint: Option<u64>,
    ) -> Self {
        Self(Arc::new(Mutex::new(State {
            evaluation_limit,
            wall_time: wall_time_ms.map(Duration::from_millis),
            replay_checkpoint,
            replay_limit: Limit::WallTime,
            checkpoints: 0,
            evaluations: 0,
            active_since: Some(Instant::now()),
            paused: false,
            cancel_requested: false,
            waiters: Vec::new(),
            elapsed: Duration::ZERO,
            stopped: None,
            now_ms: 0,
            trace: Trace::default(),
            trace_bytes: 64 * 1024 * 1024,
            checks: Vec::new(),
        })))
    }

    pub fn checkpoint(&self, evaluation: bool) -> Result<(), Limit> {
        let mut state = self.0.lock().expect("execution control");
        if let Some(limit) = &state.stopped {
            return Err(limit.clone());
        }
        state.checkpoints = state.checkpoints.saturating_add(1);
        let elapsed = state.elapsed
            + state
                .active_since
                .map_or(Duration::ZERO, |start| start.elapsed());
        let wall_reached = state.replay_checkpoint.map_or_else(
            || state.wall_time.is_some_and(|limit| elapsed >= limit),
            |checkpoint| state.checkpoints >= checkpoint,
        );
        let limit = if state.cancel_requested {
            Some(Limit::Cancelled)
        } else if wall_reached {
            Some(state.replay_limit.clone())
        } else if evaluation && state.evaluations >= state.evaluation_limit {
            Some(Limit::Evaluations)
        } else {
            None
        };
        if let Some(limit) = limit {
            state.stopped = Some(limit.clone());
            return Err(limit);
        }
        if evaluation {
            state.evaluations += 1;
        }
        Ok(())
    }

    pub fn pause(&self) {
        let mut state = self.0.lock().expect("execution control");
        state.paused = true;
        if let Some(start) = state.active_since.take() {
            state.elapsed += start.elapsed();
        }
    }

    pub fn resume(&self) {
        let waiters = {
            let mut state = self.0.lock().expect("execution control");
            state.paused = false;
            state.active_since.get_or_insert_with(Instant::now);
            std::mem::take(&mut state.waiters)
        };
        for waiter in waiters {
            waiter.wake();
        }
    }

    pub async fn wait_running(&self) {
        std::future::poll_fn(|context| {
            let mut state = self.0.lock().expect("execution control");
            if !state.paused || state.cancel_requested || state.stopped.is_some() {
                return Poll::Ready(());
            }
            if state
                .waiters
                .iter()
                .all(|waiter| !waiter.will_wake(context.waker()))
            {
                state.waiters.push(context.waker().clone());
            }
            Poll::Pending
        })
        .await
    }

    pub fn request_cancel(&self) {
        let waiters = {
            let mut state = self.0.lock().expect("execution control");
            state.cancel_requested = true;
            std::mem::take(&mut state.waiters)
        };
        for waiter in waiters {
            waiter.wake();
        }
    }

    pub fn set_replay_stop(&self, checkpoint: u64, limit: Limit) {
        let mut state = self.0.lock().expect("execution control");
        state.replay_checkpoint = Some(checkpoint);
        state.replay_limit = limit;
    }

    pub fn stop(&self, limit: Limit) {
        let waiters = {
            let mut state = self.0.lock().expect("execution control");
            state.stopped.get_or_insert(limit);
            std::mem::take(&mut state.waiters)
        };
        for waiter in waiters {
            waiter.wake();
        }
    }

    pub fn stopped(&self) -> Option<Limit> {
        self.0.lock().expect("execution control").stopped.clone()
    }

    pub fn position(&self) -> (u64, u64) {
        let state = self.0.lock().expect("execution control");
        (state.evaluations, state.checkpoints)
    }

    pub fn configure(&self, trace_bytes: u64, checks: Vec<ResolvedCheck>) {
        let mut state = self.0.lock().expect("execution control");
        state.trace_bytes = trace_bytes;
        state.checks = checks;
    }

    pub fn set_time(&self, now_ms: i64) {
        self.0.lock().expect("execution control").now_ms = now_ms;
    }

    pub fn now_ms(&self) -> i64 {
        self.0.lock().expect("execution control").now_ms
    }

    pub fn record(&self, mut step: RuleStep) -> Result<(), Limit> {
        let _ = self.checkpoint(false);
        let mut state = self.0.lock().expect("execution control");
        step.virtual_ms = state.now_ms;
        let byte_limit = state.trace_bytes;
        let cycle = match state.trace.append(step.clone(), byte_limit) {
            Ok(cycle) => cycle,
            Err(()) => {
                state.stopped.get_or_insert(Limit::Evidence);
                return Err(Limit::Evidence);
            }
        };
        let failed = state.checks.iter().find_map(|resolved| {
            let check = &resolved.definition;
            if !check.options.enabled
                || check.evaluation() != Evaluation::EveryChange
                || check
                    .options
                    .window
                    .from_ms
                    .is_some_and(|from| state.now_ms < from)
                || check
                    .options
                    .window
                    .until_ms
                    .is_some_and(|until| state.now_ms > until)
            {
                return None;
            }
            let violates = match &check.predicate {
                Predicate::NoRuleCycles {
                    include_timed_recurrence,
                } => cycle.as_ref().is_some_and(|cycle| {
                    *include_timed_recurrence || cycle.kind != CycleKind::TimedRecurrence
                }),
                Predicate::Quantity {
                    cell,
                    comparison,
                    expected,
                    ..
                } if cell == &step.cell && check.options.quantity == QuantityBasis::Stored => step
                    .changes
                    .iter()
                    .rev()
                    .find(|change| resolved.record.as_deref() == Some(change.record.as_str()))
                    .is_some_and(|change| {
                        change.after.unit == expected.unit
                            && !comparison
                                .accepts(change.after.value.exact_numeric_cmp(expected.value))
                    }),
                Predicate::QuantityEquals { cell, expected, .. }
                    if cell == &step.cell && check.options.quantity == QuantityBasis::Stored =>
                {
                    step.changes
                        .iter()
                        .rev()
                        .find(|change| resolved.record.as_deref() == Some(change.record.as_str()))
                        .is_some_and(|change| {
                            change.after.unit == expected.unit
                                && change.after.value.exact_numeric_cmp(expected.value).is_ne()
                        })
                }
                Predicate::Nonnegative { cell, .. }
                    if cell == &step.cell && check.options.quantity == QuantityBasis::Stored =>
                {
                    step.changes
                        .iter()
                        .rev()
                        .find(|change| resolved.record.as_deref() == Some(change.record.as_str()))
                        .is_some_and(|change| change.after.value.is_negative())
                }
                _ => false,
            };
            violates.then(|| check.id.clone())
        });
        if let Some(check) = failed {
            state.stopped.get_or_insert(Limit::Restriction(check));
        }
        state.stopped.clone().map_or(Ok(()), Err)
    }

    pub fn check_quantity(
        &self,
        cell: &str,
        record: &str,
        quantity: &Quantity,
    ) -> Result<(), Limit> {
        self.check_quantity_basis(cell, record, quantity, QuantityBasis::Stored)
    }

    pub fn check_available_quantity(
        &self,
        cell: &str,
        record: &str,
        quantity: &Quantity,
    ) -> Result<(), Limit> {
        self.check_quantity_basis(cell, record, quantity, QuantityBasis::Available)
    }

    pub fn checks_available(&self, cell: &str, record: &str) -> bool {
        let state = self.0.lock().expect("execution control");
        state.checks.iter().any(|resolved| {
            let check = &resolved.definition;
            check.options.enabled && check.options.quantity == QuantityBasis::Available
                && check.evaluation() == Evaluation::EveryChange
                && resolved.record.as_deref() == Some(record)
                && check.options.window.from_ms.is_none_or(|from| state.now_ms >= from)
                && check.options.window.until_ms.is_none_or(|until| state.now_ms <= until)
                && matches!(&check.predicate, Predicate::Quantity {cell:selected,..} | Predicate::QuantityEquals {cell:selected,..} | Predicate::Nonnegative {cell:selected,..} if selected == cell)
        })
    }

    fn check_quantity_basis(
        &self,
        cell: &str,
        record: &str,
        quantity: &Quantity,
        basis: QuantityBasis,
    ) -> Result<(), Limit> {
        let mut state = self.0.lock().expect("execution control");
        let failed = state.checks.iter().find_map(|resolved| {
            let check = &resolved.definition;
            if !check.options.enabled
                || check.options.quantity != basis
                || check.evaluation() != Evaluation::EveryChange
                || resolved.record.as_deref() != Some(record)
                || check
                    .options
                    .window
                    .from_ms
                    .is_some_and(|from| state.now_ms < from)
                || check
                    .options
                    .window
                    .until_ms
                    .is_some_and(|until| state.now_ms > until)
            {
                return None;
            }
            let violates = match &check.predicate {
                Predicate::Quantity {
                    cell: selected,
                    comparison,
                    expected,
                    ..
                } if selected == cell => {
                    quantity.unit == expected.unit
                        && !comparison.accepts(quantity.value.exact_numeric_cmp(expected.value))
                }
                Predicate::Nonnegative { cell: selected, .. } if selected == cell => {
                    quantity.value.is_negative()
                }
                Predicate::QuantityEquals {
                    cell: selected,
                    expected,
                    ..
                } if selected == cell => {
                    quantity.unit == expected.unit
                        && quantity.value.exact_numeric_cmp(expected.value).is_ne()
                }
                _ => false,
            };
            violates.then(|| check.id.clone())
        });
        if let Some(check) = failed {
            state.stopped.get_or_insert(Limit::Restriction(check));
        }
        state.stopped.clone().map_or(Ok(()), Err)
    }

    pub fn settle(&self, cell: &str) {
        self.0.lock().expect("execution control").trace.settle(cell);
    }

    pub fn cycles(&self) -> Vec<RuleCycle> {
        self.0.lock().expect("execution control").trace.cycles()
    }

    pub fn drain_cycles(&self, cell: &str) -> Vec<RuleCycle> {
        self.0.lock().expect("execution control").trace.drain(cell)
    }

    pub fn link_effect(&self, cell: &str, fact: &str, occurrence: &RuleOccurrence) {
        self.0
            .lock()
            .expect("execution control")
            .trace
            .link_effect(cell, fact, occurrence);
    }

    pub fn link_received(&self, cell: &str, fact: &str, original: &str) {
        self.0
            .lock()
            .expect("execution control")
            .trace
            .link_received(cell, fact, original);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_evaluation_limits_count_only_rule_evaluations() {
        let control = Control::new(2, None, None);
        let other = control.clone();
        control.checkpoint(false).unwrap();
        control.checkpoint(true).unwrap();
        other.checkpoint(true).unwrap();
        assert_eq!(control.checkpoint(true), Err(Limit::Evaluations));
        assert_eq!(control.position(), (2, 4));
        assert_eq!(other.checkpoint(false), Err(Limit::Evaluations));
        assert_eq!(control.position(), (2, 4));
    }

    #[test]
    fn replay_uses_the_recorded_boundary_instead_of_processor_speed() {
        let control = Control::new(100, Some(1), Some(3));
        control.checkpoint(true).unwrap();
        control.checkpoint(false).unwrap();
        assert_eq!(control.checkpoint(true), Err(Limit::WallTime));
        assert_eq!(control.position(), (1, 3));
    }

    #[test]
    fn paused_work_wakes_on_resume_or_stop_without_spending_evaluations() {
        use std::future::Future;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::task::{Context, Wake};

        #[derive(Default)]
        struct Counter(AtomicUsize);

        impl Wake for Counter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(Counter::default());
        let waker = Waker::from(counter.clone());
        let mut context = Context::from_waker(&waker);
        let control = Control::new(100, None, None);
        control.pause();
        let mut first = Box::pin(control.wait_running());
        assert!(first.as_mut().poll(&mut context).is_pending());
        assert_eq!(control.position(), (0, 0));
        control.resume();
        assert_eq!(counter.0.load(Ordering::SeqCst), 1);
        assert!(first.as_mut().poll(&mut context).is_ready());
        control.pause();
        let mut second = Box::pin(control.wait_running());
        assert!(second.as_mut().poll(&mut context).is_pending());
        control.stop(Limit::Evaluations);
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
        assert!(second.as_mut().poll(&mut context).is_ready());
        assert_eq!(control.position(), (0, 0));
    }
}
