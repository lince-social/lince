use std::collections::{BTreeMap, BTreeSet};

use nucleus::karma::CanonicalHash;
use nucleus::simulation::{self as report, Observation, Predicate, Witness};

use crate::Result;
use crate::world::{World, digest};

pub struct Checks {
    pub findings: Vec<report::Finding>,
    pub coverage: Vec<report::Coverage>,
    pub specifications: Vec<report::Check>,
    completed: BTreeSet<String>,
    previous_state: CanonicalHash,
    observed_events: usize,
}

impl Checks {
    pub async fn new(world: &World) -> Result<Self> {
        let implementation = digest(&include_str!("checks.rs"))?;
        let specifications: Vec<_> = world
            .scenario
            .checks
            .iter()
            .map(|check| report::Check {
                id: check.id.clone(),
                implementation: implementation.clone(),
                predicate: check.predicate.clone(),
            })
            .collect();
        Ok(Self {
            coverage: specifications
                .iter()
                .map(|check| report::Coverage {
                    check: check.id.clone(),
                    observations: 0,
                    complete: false,
                })
                .collect(),
            specifications,
            findings: Vec::new(),
            completed: BTreeSet::new(),
            previous_state: world.state_hash().await?,
            observed_events: 0,
        })
    }

    pub async fn observe(&mut self, world: &World) -> Result<()> {
        let after_state = world.state_hash().await?;
        let settled = world.next_ms().is_none_or(|next| next > world.now_ms);
        for index in 0..self.specifications.len() {
            let check = &self.specifications[index];
            if self.completed.contains(&check.id) {
                continue;
            }
            let mut evidence = Vec::new();
            let mut cells = Vec::new();
            let witness = match &check.predicate {
                Predicate::QuantityEquals {
                    cell,
                    record,
                    expected,
                    at_ms,
                } => {
                    if world.now_ms < *at_ms || !settled {
                        continue;
                    }
                    cells.push(cell.clone());
                    self.completed.insert(check.id.clone());
                    self.coverage[index].complete = true;
                    match world.quantity(cell, record).await? {
                        Some((record, observed)) if !equal(&observed, expected) => {
                            Some(Witness::Quantity {
                                record,
                                expected: expected.clone(),
                                observed,
                            })
                        }
                        Some(_) => None,
                        None => Some(Witness::MissingRecord {
                            reference: record.clone(),
                        }),
                    }
                }
                Predicate::Nonnegative { cell, record } => {
                    cells.push(cell.clone());
                    let current = world.quantity(cell, record).await?;
                    let violation = current.as_ref().and_then(|(target, _)| {
                        world.trace[self.observed_events..]
                            .iter()
                            .find_map(|event| match &event.observation {
                                Observation::CommittedQuantity { record, after, .. }
                                    if &event.cell == cell
                                        && record == target
                                        && after.value.mantissa() < 0 =>
                                {
                                    Some((event.sequence, (record.clone(), after.clone())))
                                }
                                _ => None,
                            })
                    });
                    let observed = if let Some((sequence, violation)) = violation {
                        evidence.push(sequence);
                        Some(violation)
                    } else {
                        current
                    };
                    match observed {
                        Some((record, observed)) if observed.value.mantissa() < 0 => {
                            let expected = report::Quantity {
                                value: store::exact::zero(),
                                unit: observed.unit.clone(),
                            };
                            Some(Witness::Quantity {
                                record,
                                expected,
                                observed,
                            })
                        }
                        Some(_) => None,
                        None => Some(Witness::MissingRecord {
                            reference: record.clone(),
                        }),
                    }
                }
                Predicate::Converged {
                    cells: peers,
                    record,
                    at_ms,
                } => {
                    if world.now_ms < *at_ms || !settled {
                        continue;
                    }
                    cells = peers.clone();
                    let mut values = Vec::new();
                    for cell in peers {
                        if let Some((_, quantity)) = world.quantity(cell, record).await? {
                            values.push(report::CellQuantity {
                                cell: cell.clone(),
                                quantity,
                            });
                        }
                    }
                    self.completed.insert(check.id.clone());
                    self.coverage[index].complete = true;
                    let converged = values.len() == peers.len()
                        && values
                            .iter()
                            .all(|value| equal(&value.quantity, &values[0].quantity));
                    (!converged).then(|| Witness::DivergentCells {
                        record: world.resolve_reference(record),
                        values,
                        deadline_ms: *at_ms,
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
                    world.trace[self.observed_events..].iter().find_map(|event| {
                        let witness = match &event.observation {
                            Observation::ActionRefused { input, refusal } => {
                                if self.specifications.iter().any(|check| matches!(&check.predicate, Predicate::ExpectedRefusal { input: expected, refusal: expected_refusal } if expected == input && expected_refusal == refusal)) { return None; }
                                Witness::RefusedAction { input: input.clone(), refusal: refusal.clone() }
                            }
                            Observation::MessageRefused { message, refusal, .. } => {
                                if self.specifications.iter().any(|check| matches!(&check.predicate, Predicate::ExpectedMessageRefusal { input, copy, refusal: expected } if expected == refusal && queued_message(world, input, *copy) == Some(*message))) { return None; }
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
                    let mut applications = BTreeMap::<_, Vec<u64>>::new();
                    let mut commits = BTreeMap::<_, Vec<u64>>::new();
                    let mut found = None;
                    for event in &world.trace {
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
                        let mut previous = "genesis".to_owned();
                        let mut position = 0;
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
                        if found.is_some() {
                            break;
                        }
                    }
                    found
                }
            };
            self.coverage[index].observations += 1;
            if let Some(witness) = witness {
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
                                        | Observation::ActionRefused { .. }
                                )
                        })
                        .take(16)
                        .map(|event| event.sequence)
                        .collect();
                    evidence.reverse();
                }
                self.completed.insert(check.id.clone());
                self.findings.push(report::Finding {
                    check: check.clone(),
                    sequence: world.trace.len().saturating_sub(1) as u64,
                    virtual_ms: world.now_ms,
                    cells,
                    witness,
                    evidence,
                    before_state: self.previous_state.clone(),
                    after_state: after_state.clone(),
                });
            }
        }
        self.previous_state = after_state;
        self.observed_events = world.trace.len();
        Ok(())
    }

    pub async fn finish(&mut self, world: &World) -> Result<report::Result> {
        self.observe(world).await?;
        let stop = world.stop.clone().unwrap_or(report::Stop::Paused {});
        if stop == (report::Stop::HorizonReached {}) {
            for (specification, coverage) in self.specifications.iter().zip(&mut self.coverage) {
                if matches!(
                    specification.predicate,
                    Predicate::Nonnegative { .. }
                        | Predicate::FactChain {}
                        | Predicate::OncePerOccurrence {}
                        | Predicate::NoUnexpectedRefusals {}
                ) {
                    coverage.complete = true;
                }
            }
        }
        let verdict = if !self.findings.is_empty() {
            report::Verdict::Failed
        } else if stop == (report::Stop::HorizonReached {})
            && self.coverage.iter().all(|coverage| coverage.complete)
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
