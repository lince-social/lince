mod quantity;

use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::CanonicalHash;
use nucleus::simulation::{
    self as report, CheckStatus, Comparison, CoverageKind, CoverageReason, Evaluation, FailureMode,
    Observation, Predicate, Witness,
};

use crate::Result;
use crate::world::{World, digest};

pub struct Checks {
    pub findings: Vec<report::Finding>,
    pub coverage: Vec<report::Coverage>,
    pub specifications: Vec<report::Check>,
    completed: BTreeSet<String>,
    previous_state: CanonicalHash,
    runtime: Vec<Runtime>,
    evaluations: u64,
    pub costs: Vec<report::checks::CheckCost>,
}

#[derive(Default)]
struct Runtime {
    cursor: usize,
    quantity_cursor: usize,
    quantity: quantity::State,
    last: Option<(u64, i64)>,
    chains: BTreeMap<String, (i64, String)>,
    commits: BTreeMap<(String, String, u64, String, u32), Vec<u64>>,
    applications: BTreeMap<(String, String, u64, String), Vec<u64>>,
}

impl Checks {
    pub async fn new(world: &World) -> Result<Self> {
        let implementation = digest(&(
            include_str!("checks.rs"),
            include_str!("checks/quantity.rs"),
            include_str!("../../nucleus/src/simulation/checks.rs"),
        ))?;
        let specifications: Vec<_> = world
            .scenario
            .checks
            .iter()
            .map(|check| report::Check {
                id: check.id.clone(),
                implementation: implementation.clone(),
                predicate: check.predicate.clone(),
                options: check.options.clone(),
            })
            .collect();
        Ok(Self {
            coverage: specifications
                .iter()
                .zip(&world.scenario.checks)
                .map(|(check, definition)| report::Coverage {
                    check: check.id.clone(),
                    observations: 0,
                    complete: false,
                    status: if check.options.enabled {
                        CheckStatus::Incomplete
                    } else {
                        CheckStatus::Skipped
                    },
                    kind: if matches!(
                        check.predicate,
                        Predicate::FactChain {}
                            | Predicate::OncePerOccurrence {}
                            | Predicate::NoUnexpectedRefusals {}
                            | Predicate::ExpectedRefusal { .. }
                            | Predicate::ExpectedMessageRefusal { .. }
                            | Predicate::NoRuleCycles { .. }
                    ) {
                        CoverageKind::History
                    } else {
                        match definition.evaluation() {
                            Evaluation::EveryEvents { .. } | Evaluation::EveryDuration { .. } => {
                                CoverageKind::Sampled
                            }
                            Evaluation::EveryChange => CoverageKind::Continuous,
                            _ => CoverageKind::Instant,
                        }
                    },
                    from_ms: definition
                        .interval(world.scenario.start_ms, world.scenario.end_ms)
                        .0,
                    until_ms: definition
                        .interval(world.scenario.start_ms, world.scenario.end_ms)
                        .1,
                    last_evaluated_ms: None,
                    reason: None,
                })
                .collect(),
            costs: specifications
                .iter()
                .map(|check| report::checks::CheckCost {
                    check: check.id.clone(),
                    ..Default::default()
                })
                .collect(),
            runtime: specifications
                .iter()
                .map(|_| Runtime {
                    quantity_cursor: world.trace.len(),
                    ..Default::default()
                })
                .collect(),
            specifications,
            findings: Vec::new(),
            completed: BTreeSet::new(),
            previous_state: world.state_hash().await?,
            evaluations: 0,
        })
    }

    pub async fn observe(&mut self, world: &mut World) -> Result<()> {
        let settled = world
            .domain_next_ms()
            .is_none_or(|next| next > world.now_ms);
        let mut after_state = None;
        for index in 0..self.specifications.len() {
            let check = &self.specifications[index];
            let definition = &world.scenario.checks[index];
            let evaluation = definition.evaluation();
            let coverage = &self.coverage[index];
            let runtime = &mut self.runtime[index];
            if !check.options.enabled
                || self.completed.contains(&check.id)
                || world.now_ms < coverage.from_ms
                || world.now_ms > coverage.until_ms
            {
                continue;
            }
            let last = runtime.last;
            let ready = match evaluation {
                Evaluation::Default => unreachable!(),
                Evaluation::End => world.now_ms >= coverage.until_ms && settled,
                Evaluation::At { at_ms } => world.now_ms >= at_ms && settled,
                Evaluation::EveryChange => {
                    last.is_none_or(|(steps, _)| steps != world.steps)
                        || world.now_ms == coverage.until_ms && settled
                }
                Evaluation::EveryEvents { every } => {
                    last.is_none()
                        || world.now_ms == coverage.until_ms && settled
                        || last.is_some_and(|(steps, _)| world.steps >= steps.saturating_add(every))
                }
                Evaluation::EveryDuration { millis } => {
                    settled
                        && (world.now_ms == coverage.until_ms
                            || (world.now_ms - coverage.from_ms) as u64 % millis == 0)
                }
            };
            if !ready || last == Some((world.steps, world.now_ms)) {
                continue;
            }
            if evaluation == Evaluation::EveryChange && world.now_ms != coverage.until_ms {
                let target = match &check.predicate {
                    Predicate::Quantity { cell, .. }
                    | Predicate::QuantityEquals { cell, .. }
                    | Predicate::Nonnegative { cell, .. } => Some(cell),
                    _ => None,
                };
                if target.is_some_and(|cell| {
                    !runtime
                        .quantity
                        .changed(world, cell, runtime.quantity_cursor)
                }) {
                    runtime.quantity_cursor = world.trace.len();
                    runtime.last = Some((world.steps, world.now_ms));
                    continue;
                }
            }
            if self.evaluations >= world.scenario.checking.evaluations {
                self.coverage[index].reason = Some(CoverageReason::CheckBudget);
                world.stop = Some(report::Stop::CheckBudget {});
                break;
            }
            if after_state.is_none() {
                after_state = Some(world.state_hash().await?);
            }
            let started = std::time::Instant::now();
            let mut evaluations = 1;
            let observed_events = runtime.cursor;
            runtime.last = Some((world.steps, world.now_ms));
            let mut evidence = Vec::new();
            let mut cells = Vec::new();
            let witness = match &check.predicate {
                Predicate::NoRuleCycles { include_timed_recurrence } => {
                    world.trace[observed_events..].iter().find_map(|event| {
                        if let Observation::RuleCycle { cycle } = &event.observation
                            && (*include_timed_recurrence || cycle.kind != report::CycleKind::TimedRecurrence) {
                            cells.extend(cycle.steps.iter().map(|step| step.cell.clone()));
                            cells.sort();
                            cells.dedup();
                            evidence.push(event.sequence);
                            return Some(Witness::RuleCycle { cycle: cycle.clone() });
                        }
                        None
                    })
                }
                Predicate::Quantity { cell, record, expected, .. }
                | Predicate::QuantityEquals { cell, record, expected, .. } => {
                    cells.push(cell.clone());
                    let comparison = match check.predicate { Predicate::Quantity { comparison, .. } => comparison, _ => Comparison::Equal };
                    let outcome = quantity::evaluate(&mut runtime.quantity, world, quantity::Request {
                        basis:check.options.quantity,
                        cell, reference: record, expected: Some(expected), comparison,
                        continuous: evaluation == Evaluation::EveryChange,
                        since: runtime.quantity_cursor,
                        budget: world.scenario.checking.evaluations - self.evaluations,
                    }).await?;
                    evaluations = outcome.evaluations;
                    evidence = outcome.evidence;
                    if outcome.reason.is_some() { self.coverage[index].reason = outcome.reason; }
                    outcome.witness
                }
                Predicate::Nonnegative { cell, record } => {
                    cells.push(cell.clone());
                    let outcome = quantity::evaluate(&mut runtime.quantity, world, quantity::Request {
                        basis:check.options.quantity,
                        cell, reference: record, expected: None, comparison: Comparison::AtLeast,
                        continuous: evaluation == Evaluation::EveryChange,
                        since: runtime.quantity_cursor,
                        budget: world.scenario.checking.evaluations - self.evaluations,
                    }).await?;
                    evaluations = outcome.evaluations;
                    evidence = outcome.evidence;
                    if outcome.reason.is_some() { self.coverage[index].reason = outcome.reason; }
                    outcome.witness
                }
                Predicate::Converged {
                    cells: peers,
                    record,
                    ..
                } => {
                    cells = peers.clone();
                    let mut values = Vec::new();
                    let mut unsupported_unit = false;
                    for cell in peers {
                        unsupported_unit |= check.options.quantity == report::QuantityBasis::Available && world.available_unit_changed(cell, record).await?;
                        if let Some((_, quantity)) = world.quantity_with_basis(cell, record,check.options.quantity).await? {
                            values.push(report::CellQuantity {
                                cell: cell.clone(),
                                quantity,
                            });
                        }
                    }
                    let converged = values.len() == peers.len()
                        && values
                            .iter()
                            .all(|value| equal(&value.quantity, &values[0].quantity));
                    if unsupported_unit { self.coverage[index].reason = Some(report::CoverageReason::UnsupportedUnit); }
                    (!converged && !unsupported_unit).then(|| Witness::DivergentCells {
                        record: world.resolve_reference(record),
                        values,
                        deadline_ms: world.now_ms,
                    })
                }
                Predicate::ExpectedRefusal { input, refusal } => {
                    let result = world
                        .trace
                        .iter()
                        .find_map(|event| match &event.observation {
                            Observation::ActionAccepted { input: found, .. } if found == input => {
                                Some((event, None))
                            }
                            Observation::ActionRefused {
                                input: found,
                                refusal,
                            } if found == input => Some((event, Some(refusal.clone()))),
                            _ => None,
                        });
                    let Some((event, observed)) = result else {
                        continue;
                    };
                    cells.push(event.cell.clone());
                    evidence.push(event.sequence);
                    self.completed.insert(check.id.clone());
                    self.coverage[index].complete = true;
                    (observed.as_ref() != Some(refusal)).then(|| Witness::UnexpectedActionResult {
                        input: input.clone(),
                        expected: refusal.clone(),
                        observed,
                    })
                }
                Predicate::ExpectedMessageRefusal { input, copy, refusal } => {
                    let Some(message) = queued_message(world, input, *copy) else {
                        continue;
                    };
                    let result = world.trace.iter().find_map(|event| {
                        match &event.observation {
                            Observation::MessageRefused { message: found, refusal, .. }
                                if *found == message => Some((event, Some(refusal.clone()))),
                            Observation::MessageDelivered { message: found, .. }
                            | Observation::TransferReceived { message: found, .. }
                                if *found == message => Some((event, None)),
                            _ => None,
                        }
                    });
                    let Some((event, observed)) = result else {
                        continue;
                    };
                    cells.push(event.cell.clone());
                    evidence.push(event.sequence);
                    self.completed.insert(check.id.clone());
                    self.coverage[index].complete = true;
                    (observed.as_ref() != Some(refusal)).then(|| Witness::UnexpectedMessageResult {
                        message,
                        expected: refusal.clone(),
                        observed,
                    })
                }
                Predicate::NoUnexpectedRefusals {} => {
                    world.trace[observed_events..].iter().find_map(|event| {
                        let witness = match &event.observation {
                            Observation::ActionRefused { input, refusal } => {
                                if self.specifications.iter().filter(|check| check.options.enabled).any(|check| matches!(&check.predicate, Predicate::ExpectedRefusal { input: expected, refusal: expected_refusal } if expected == input && expected_refusal == refusal)) { return None; }
                                Witness::RefusedAction { input: input.clone(), refusal: refusal.clone() }
                            }
                            Observation::MessageRefused { message, refusal, .. } => {
                                if self.specifications.iter().filter(|check| check.options.enabled).any(|check| matches!(&check.predicate, Predicate::ExpectedMessageRefusal { input, copy, refusal: expected } if expected == refusal && queued_message(world, input, *copy) == Some(*message))) { return None; }
                                Witness::RefusedMessage { message: *message, refusal: refusal.clone() }
                            },
                            Observation::DatabaseEffect { uid, effect, ok: false, result } => Witness::RefusedEffect { uid: uid.clone(), effect: *effect, reason: result.clone() },
                            Observation::RuleApplication { occurrence, status: report::ApplicationStatus::Failed, reason } => Witness::RefusedRule { occurrence: occurrence.clone(), reason: reason.clone() },
                            _ => return None,
                        };
                        cells.push(event.cell.clone());
                        evidence.push(event.sequence);
                        Some(witness)
                    })
                }
                Predicate::OncePerOccurrence {} => {
                    let applications = &mut runtime.applications;
                    let commits = &mut runtime.commits;
                    let mut found = None;
                    for event in &world.trace[observed_events..] {
                        if let Observation::CommittedQuantity {
                            cause:
                                report::Cause::Rule {
                                    occurrence,
                                    consequence,
                                },
                            ..
                        } = &event.observation
                        {
                            let positions = commits
                                .entry((
                                    event.cell.clone(),
                                    occurrence.rule_uid.clone(),
                                    occurrence.revision,
                                    occurrence.event_id.clone(),
                                    *consequence,
                                ))
                                .or_default();
                            positions.push(event.sequence);
                            if positions.len() > 1 {
                                cells.push(event.cell.clone());
                                evidence = positions.clone();
                                found = Some(Witness::DuplicateApplication {
                                    occurrence: occurrence.clone(),
                                    consequence: Some(*consequence),
                                    expected: 1,
                                    observed: positions.len() as u64,
                                    commits: positions.clone(),
                                });
                                break;
                            }
                        }
                        if let Observation::RuleApplication {
                            occurrence,
                            status: report::ApplicationStatus::Applied,
                            ..
                        } = &event.observation
                        {
                            let positions = applications
                                .entry((
                                    event.cell.clone(),
                                    occurrence.rule_uid.clone(),
                                    occurrence.revision,
                                    occurrence.event_id.clone(),
                                ))
                                .or_default();
                            positions.push(event.sequence);
                            if positions.len() > 1 {
                                cells.push(event.cell.clone());
                                evidence = positions.clone();
                                found = Some(Witness::DuplicateApplication {
                                    occurrence: occurrence.clone(),
                                    consequence: None,
                                    expected: 1,
                                    observed: positions.len() as u64,
                                    commits: positions.clone(),
                                });
                                break;
                            }
                        }
                    }
                    found
                }
                Predicate::FactChain {} => {
                    let mut found = None;
                    for (cell, node) in &world.nodes {
                        let (mut position, mut previous) = runtime.chains.get(cell).cloned().unwrap_or((0, "genesis".to_owned()));
                        loop {
                            let facts = store::facts::after_position(
                                &node.engine().store.pool,
                                position,
                                256,
                            )
                            .await?;
                            if facts.is_empty() {
                                break;
                            }
                            for (at, fact) in facts {
                                if fact.prev_hash != previous
                                    || !nucleus::fact::verify_chain_step(&fact)
                                {
                                    cells.push(cell.clone());
                                    found = Some(Witness::BrokenFactChain {
                                        fact: fact.uid.try_into()?,
                                        previous: fact.prev_hash,
                                        expected: previous.clone(),
                                    });
                                    break;
                                }
                                previous = fact.hash;
                                position = at;
                            }
                            if found.is_some() {
                                break;
                            }
                        }
                        runtime.chains.insert(cell.clone(), (position, previous));
                        if found.is_some() {
                            break;
                        }
                    }
                    found
                }
            };
            runtime.cursor = world.trace.len();
            runtime.quantity_cursor = world.trace.len();
            self.coverage[index].observations += evaluations;
            self.coverage[index].last_evaluated_ms = Some(world.now_ms);
            self.evaluations += evaluations;
            self.costs[index].evaluations += evaluations;
            self.costs[index].micros +=
                u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            if matches!(evaluation, Evaluation::End | Evaluation::At { .. }) {
                self.coverage[index].complete = true;
                self.completed.insert(check.id.clone());
            }
            if self.coverage[index].reason == Some(CoverageReason::CheckBudget) {
                world.stop = Some(report::Stop::CheckBudget {});
            }
            if let Some(witness) = witness
                && self.coverage[index].status != CheckStatus::Failed
            {
                if evidence.is_empty() {
                    evidence = world
                        .trace
                        .iter()
                        .rev()
                        .filter(|event| {
                            cells.contains(&event.cell)
                                && matches!(
                                    event.observation,
                                    Observation::CommittedQuantity { .. }
                                        | Observation::LoanQuantity { .. }
                                        | Observation::ActionRefused { .. }
                                )
                        })
                        .take(16)
                        .map(|event| event.sequence)
                        .collect();
                    evidence.reverse();
                }
                self.coverage[index].status = CheckStatus::Failed;
                self.findings.push(report::Finding {
                    check: check.clone(),
                    sequence: world.trace.len().saturating_sub(1) as u64,
                    virtual_ms: world.now_ms,
                    cells,
                    witness,
                    evidence,
                    before_state: self.previous_state.clone(),
                    after_state: after_state.clone().unwrap(),
                });
                if world.scenario.checking.on_failure == FailureMode::Stop {
                    world.stop = Some(report::Stop::CheckFailed {
                        check: check.id.clone(),
                    });
                    break;
                }
            }
        }
        if let Some(after_state) = after_state {
            self.previous_state = after_state;
        }
        Ok(())
    }

    pub async fn finish(&mut self, world: &mut World) -> Result<report::Result> {
        if world.stop == Some(report::Stop::HorizonReached {}) {
            self.observe(world).await?;
        }
        let stop = world.stop.clone().unwrap_or(report::Stop::Paused {});
        for (specification, coverage) in self.specifications.iter().zip(&mut self.coverage) {
            if !specification.options.enabled {
                continue;
            }
            if stop == (report::Stop::HorizonReached {})
                && coverage.observations > 0
                && coverage.reason.is_none()
                && (coverage.complete || coverage.last_evaluated_ms == Some(coverage.until_ms))
                && !matches!(
                    specification.predicate,
                    Predicate::ExpectedRefusal { .. } | Predicate::ExpectedMessageRefusal { .. }
                )
            {
                coverage.complete = true;
            }
            if coverage.reason.is_some() {
                coverage.complete = false;
            }
            if coverage.status != CheckStatus::Failed {
                if coverage.complete {
                    coverage.status = CheckStatus::Passed;
                } else {
                    coverage.reason.get_or_insert(CoverageReason::Stopped);
                }
            }
        }
        let verdict = if !self.findings.is_empty() {
            report::Verdict::Failed
        } else if self
            .specifications
            .iter()
            .all(|check| !check.options.enabled)
        {
            report::Verdict::Unverified
        } else if stop == (report::Stop::HorizonReached {})
            && self.coverage.iter().all(|coverage| {
                matches!(coverage.status, CheckStatus::Passed | CheckStatus::Skipped)
            })
        {
            report::Verdict::Passed
        } else {
            report::Verdict::Inconclusive
        };
        Ok(report::Result {
            version: report::Version::V1,
            stop,
            verdict,
            stopped_at_ms: world.now_ms,
            steps: world.steps,
            rule_evaluations: world.control.position().0,
            execution_checkpoints: world.control.position().1,
            execution_interrupted: world.control.stopped().is_some(),
            cycles: world.control.cycles(),
            inputs: world.inputs,
            events: world.trace.len() as u64,
            findings: self.findings.len() as u64,
            coverage: self.coverage.clone(),
            final_state: world.state_hash().await?,
        })
    }
}

fn queued_message(world: &World, input: &str, copy: u8) -> Option<u64> {
    world
        .trace
        .iter()
        .filter_map(|event| match (&event.caused_by, &event.observation) {
            (report::Cause::Input { id }, Observation::MessageQueued { message, .. })
                if id == input =>
            {
                Some(*message)
            }
            _ => None,
        })
        .nth(usize::from(copy))
}

fn equal(left: &report::Quantity, right: &report::Quantity) -> bool {
    left.unit == right.unit
        && left
            .value
            .aligned_sub(right.value)
            .is_some_and(|delta| delta.is_zero())
}
