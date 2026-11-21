//! Executes one bounded interface check against a renderer-independent backend.

use std::time::{Duration, Instant};

use super::{
    Backend, Cancellation, Dispatch, Event, Limits, Metrics, Outcome, Report, Route, Step,
};

/// Runs interface checks using the cancellation signal active at construction.
#[derive(Debug)]
pub struct Runner {
    /// Time and poll budgets for each step.
    pub limits: Limits,
    cancellation: Cancellation,
}

impl Runner {
    /// Creates a runner with the supplied budgets.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            cancellation: Cancellation::current(),
        }
    }

    /// Returns a handle that can stop this runner.
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    /// Stops this runner and the other work sharing its signal.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    fn remaining(&self, started: Instant) -> Result<Duration, Outcome> {
        if self.cancellation.is_cancelled() {
            return Err(Outcome::Cancelled);
        }
        let remaining = self.limits.timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            Err(Outcome::Timeout)
        } else {
            Ok(remaining)
        }
    }

    /// Executes one step and collects its outcome, events, and backend diagnostics.
    pub fn run<B: Backend>(
        &mut self,
        backend: &mut B,
        step: &Step<B::Action, B::Condition>,
    ) -> Report {
        let started = Instant::now();
        let mut metrics = Metrics::default();
        let mut events = Vec::new();
        let outcome = self
            .execute(backend, step, started, &mut metrics, &mut events)
            .err()
            .unwrap_or(Outcome::Passed);
        let diagnostics = backend.diagnostics();
        backend.finish();
        events.push(Event::Finished(outcome.clone()));
        metrics.events = events.len();
        metrics.elapsed = started.elapsed();
        Report {
            step: step.name.clone(),
            selector: step.selector.clone(),
            outcome,
            metrics,
            events,
            diagnostics,
        }
    }

    fn snapshot<B: Backend>(
        &self,
        backend: &mut B,
        started: Instant,
        metrics: &mut Metrics,
        events: &mut Vec<Event>,
    ) -> Result<B::Snapshot, Outcome> {
        let remaining = self.remaining(started)?;
        metrics.snapshots += 1;
        events.push(Event::Snapshot);
        let snapshot = backend.snapshot(remaining).map_err(Outcome::BackendError)?;
        self.remaining(started)?;
        Ok(snapshot)
    }

    fn execute<B: Backend>(
        &self,
        backend: &mut B,
        step: &Step<B::Action, B::Condition>,
        started: Instant,
        metrics: &mut Metrics,
        events: &mut Vec<Event>,
    ) -> Result<(), Outcome> {
        self.remaining(started)?;
        if self.limits.max_polls == 0 {
            return Err(Outcome::PollLimit);
        }
        let mut snapshot = self.snapshot(backend, started, metrics, events)?;
        if let Some(action) = &step.action {
            for route in [Route::Primary, Route::Fallback] {
                let remaining = self.remaining(started)?;
                metrics.dispatches += 1;
                let result = backend.dispatch(&snapshot, &step.selector, action, route, remaining);
                let failure = match result {
                    Dispatch::Accepted => None,
                    Dispatch::Rejected(_) => Some(Outcome::Rejected),
                    Dispatch::Unavailable(_) => Some(Outcome::Unavailable),
                };
                events.push(Event::Dispatch { route, result });
                self.remaining(started)?;
                let Some(failure) = failure else { break };
                if route == Route::Fallback || !step.fallback {
                    return Err(failure);
                }
            }
            snapshot = self.snapshot(backend, started, metrics, events)?;
        }
        loop {
            self.remaining(started)?;
            metrics.polls += 1;
            let matched = backend
                .condition(&snapshot, &step.condition)
                .map_err(Outcome::BackendError)?;
            events.push(Event::Condition { matched });
            self.remaining(started)?;
            if matched {
                return Ok(());
            }
            if metrics.polls >= self.limits.max_polls {
                return Err(Outcome::PollLimit);
            }
            let remaining = self.remaining(started)?;
            metrics.pumps += 1;
            events.push(Event::Pump);
            backend.pump(remaining).map_err(Outcome::BackendError)?;
            self.remaining(started)?;
            snapshot = self.snapshot(backend, started, metrics, events)?;
        }
    }
}
