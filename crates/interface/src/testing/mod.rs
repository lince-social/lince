use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[cfg(feature = "native-presentation")]
pub mod ui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub timeout: Duration,
    pub max_polls: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(2),
            max_polls: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector {
    pub name: String,
    pub role: Option<String>,
}

impl Selector {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            role: None,
        }
    }

    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Primary,
    Fallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dispatch {
    Accepted,
    Rejected(String),
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Rejected,
    Unavailable,
    Timeout,
    PollLimit,
    Cancelled,
    BackendError(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Snapshot,
    Dispatch { route: Route, result: Dispatch },
    Condition { matched: bool },
    Pump,
    Finished(Outcome),
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub events: usize,
    pub pumps: usize,
    pub polls: usize,
    pub snapshots: usize,
    pub dispatches: usize,
    pub elapsed: Duration,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub step: String,
    pub selector: Selector,
    pub outcome: Outcome,
    pub metrics: Metrics,
    pub events: Vec<Event>,
    pub diagnostics: String,
}

impl Report {
    pub fn is_success(&self) -> bool {
        self.outcome == Outcome::Passed
    }
}

#[derive(Clone, Debug, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug)]
pub struct Step<A, C> {
    pub name: String,
    pub selector: Selector,
    pub action: Option<A>,
    pub condition: C,
    pub fallback: bool,
}

impl<A, C> Step<A, C> {
    pub fn action(name: impl Into<String>, selector: Selector, action: A, condition: C) -> Self {
        Self {
            name: name.into(),
            selector,
            action: Some(action),
            condition,
            fallback: false,
        }
    }

    pub fn observe(name: impl Into<String>, selector: Selector, condition: C) -> Self {
        Self {
            name: name.into(),
            selector,
            action: None,
            condition,
            fallback: false,
        }
    }

    pub fn with_fallback(mut self) -> Self {
        self.fallback = true;
        self
    }
}

pub trait Backend {
    type Snapshot;
    type Action;
    type Condition;

    fn snapshot(&mut self, remaining: Duration) -> Result<Self::Snapshot, String>;
    fn dispatch(
        &mut self,
        snapshot: &Self::Snapshot,
        selector: &Selector,
        action: &Self::Action,
        route: Route,
        remaining: Duration,
    ) -> Dispatch;
    fn condition(
        &self,
        snapshot: &Self::Snapshot,
        condition: &Self::Condition,
    ) -> Result<bool, String>;
    fn pump(&mut self, remaining: Duration) -> Result<(), String>;

    fn diagnostics(&self) -> String {
        String::new()
    }

    fn finish(&mut self) {}
}

#[derive(Debug)]
pub struct Runner {
    pub limits: Limits,
    cancellation: Cancellation,
}

impl Runner {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            cancellation: Cancellation::default(),
        }
    }

    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

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
            .unwrap_or_else(|outcome| outcome);
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
    ) -> Result<Outcome, Outcome> {
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
                events.push(Event::Dispatch {
                    route,
                    result: result.clone(),
                });
                self.remaining(started)?;
                match result {
                    Dispatch::Accepted => break,
                    Dispatch::Rejected(_) | Dispatch::Unavailable(_)
                        if route == Route::Primary && step.fallback =>
                    {
                        continue;
                    }
                    Dispatch::Rejected(_) => return Err(Outcome::Rejected),
                    Dispatch::Unavailable(_) => return Err(Outcome::Unavailable),
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
                return Ok(Outcome::Passed);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        primary: Dispatch,
        routes: Vec<Route>,
        ready: bool,
        advance: bool,
        cancellation: Option<Cancellation>,
        finishes: usize,
    }

    impl Backend for Fake {
        type Snapshot = bool;
        type Action = ();
        type Condition = ();

        fn snapshot(&mut self, remaining: Duration) -> Result<bool, String> {
            assert!(!remaining.is_zero());
            Ok(self.ready)
        }

        fn dispatch(
            &mut self,
            _: &bool,
            _: &Selector,
            _: &(),
            route: Route,
            _: Duration,
        ) -> Dispatch {
            self.routes.push(route);
            if route == Route::Primary {
                self.primary.clone()
            } else {
                Dispatch::Accepted
            }
        }

        fn condition(&self, snapshot: &bool, _: &()) -> Result<bool, String> {
            Ok(*snapshot)
        }

        fn pump(&mut self, _: Duration) -> Result<(), String> {
            self.ready = self.advance;
            if let Some(cancellation) = &self.cancellation {
                cancellation.cancel();
            }
            Ok(())
        }

        fn diagnostics(&self) -> String {
            "scoped diagnostic".into()
        }

        fn finish(&mut self) {
            self.finishes += 1;
        }
    }

    fn fake(primary: Dispatch) -> Fake {
        Fake {
            primary,
            routes: Vec::new(),
            ready: false,
            advance: true,
            cancellation: None,
            finishes: 0,
        }
    }

    fn step() -> Step<(), ()> {
        Step::action("open transport", Selector::new("Transport"), (), ()).with_fallback()
    }

    #[test]
    fn rejected_or_missing_primary_falls_back_once_then_observes() {
        for primary in [
            Dispatch::Rejected("refused".into()),
            Dispatch::Unavailable("missing".into()),
        ] {
            let mut backend = fake(primary);
            let report = Runner::new(Limits::default()).run(&mut backend, &step());
            assert!(report.is_success());
            assert_eq!(backend.finishes, 1);
            assert_eq!(backend.routes, [Route::Primary, Route::Fallback]);
            assert_eq!(
                (
                    report.metrics.dispatches,
                    report.metrics.pumps,
                    report.metrics.polls
                ),
                (2, 1, 2)
            );
        }
    }

    #[test]
    fn accepted_action_is_never_replayed_when_postcondition_stays_missing() {
        let mut backend = fake(Dispatch::Accepted);
        backend.advance = false;
        let report = Runner::new(Limits {
            max_polls: 3,
            ..Limits::default()
        })
        .run(&mut backend, &step());
        assert_eq!(report.outcome, Outcome::PollLimit);
        assert_eq!(backend.finishes, 1);
        assert_eq!(backend.routes, [Route::Primary]);
        assert_eq!(
            (
                report.metrics.dispatches,
                report.metrics.pumps,
                report.metrics.polls
            ),
            (1, 2, 3)
        );
        assert_eq!(report.diagnostics, "scoped diagnostic");
    }

    #[test]
    fn absent_explicit_fallback_fails_without_pumping_and_next_step_can_run() {
        let mut backend = fake(Dispatch::Unavailable("missing".into()));
        let mut runner = Runner::new(Limits::default());
        let mut first = step();
        first.fallback = false;
        let report = runner.run(&mut backend, &first);
        assert_eq!(report.outcome, Outcome::Unavailable);
        assert_eq!(report.metrics.pumps, 0);
        backend.primary = Dispatch::Accepted;
        assert!(runner.run(&mut backend, &step()).is_success());
        assert_eq!(backend.finishes, 2);
    }

    #[test]
    fn zero_budget_and_explicit_cancellation_never_dispatch() {
        let mut backend = fake(Dispatch::Accepted);
        for limits in [
            Limits {
                timeout: Duration::ZERO,
                ..Limits::default()
            },
            Limits {
                max_polls: 0,
                ..Limits::default()
            },
        ] {
            let report = Runner::new(limits).run(&mut backend, &step());
            assert!(!report.is_success());
            assert_eq!(report.metrics.snapshots, 0);
            assert_eq!(report.metrics.dispatches, 0);
        }
        let mut runner = Runner::new(Limits::default());
        runner.cancel();
        assert_eq!(
            runner.run(&mut backend, &step()).outcome,
            Outcome::Cancelled
        );
        assert!(backend.routes.is_empty());
        assert_eq!(backend.finishes, 3);
    }

    #[test]
    fn cancellation_during_pump_stops_before_another_snapshot_or_dispatch() {
        let mut backend = fake(Dispatch::Accepted);
        let mut runner = Runner::new(Limits::default());
        backend.cancellation = Some(runner.cancellation());
        let report = runner.run(&mut backend, &step());
        assert_eq!(report.outcome, Outcome::Cancelled);
        assert_eq!(backend.finishes, 1);
        assert_eq!(
            (
                report.metrics.dispatches,
                report.metrics.snapshots,
                report.metrics.pumps
            ),
            (1, 2, 1)
        );
    }
}
