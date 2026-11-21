// non-lean-yet

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

    fn dispatch(&mut self, _: &bool, _: &Selector, _: &(), route: Route, _: Duration) -> Dispatch {
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

#[cfg(feature = "test-driver-async")]
#[tokio::test]
async fn scoped_runner_shares_cancellation_and_restores_the_outer_signal() {
    use std::{future::ready, pin::pin};

    let outer = Cancellation::default();
    let signal = Cancellation::default();
    let runner = outer.scope(|| {
        let runner = signal.scope(|| Runner::new(Limits::default()));
        Cancellation::current().cancel();
        runner
    });
    assert!(outer.is_cancelled(), "the outer scope was restored");
    assert!(!signal.is_cancelled(), "the inner scope has its own signal");

    let mut wait = pin!(signal.cancelled());
    tokio::select! {
        biased;
        () = &mut wait => panic!("the runner has not cancelled its signal"),
        () = ready(()) => {}
    }
    runner.cancel();
    tokio::time::timeout(Duration::from_secs(1), wait)
        .await
        .expect("the runner wakes awaiters on the signal captured in its scope");
}
