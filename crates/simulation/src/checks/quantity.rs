use std::collections::BTreeMap;

use nucleus::karma::TypedUid;
use nucleus::simulation::{Comparison, CoverageReason, Observation, Quantity, Witness};

use crate::world::World;

#[derive(Default)]
pub(super) struct State {
    basis: nucleus::simulation::QuantityBasis,
    target: Option<TypedUid>,
    previous: Option<Quantity>,
}

impl State {
    pub(super) fn changed(&self, world: &World, cell: &str, since: usize) -> bool {
        let Some(target) = &self.target else {
            return true;
        };
        let current = world.observed_quantity(cell, target.as_str(), self.basis);
        if current
            .zip(self.previous.as_ref())
            .is_none_or(|(current, previous)| !super::equal(current, previous))
        {
            return true;
        }
        world.trace[since..].iter().any(|event| event.cell == cell && matches!(&event.observation, Observation::CommittedQuantity { record, .. } | Observation::LoanQuantity {record, ..} if record == target))
    }
}

pub(super) struct Outcome {
    pub witness: Option<Witness>,
    pub evidence: Vec<u64>,
    pub reason: Option<CoverageReason>,
    pub evaluations: u64,
}

pub(super) struct Request<'a> {
    pub basis: nucleus::simulation::QuantityBasis,
    pub cell: &'a str,
    pub reference: &'a str,
    pub expected: Option<&'a Quantity>,
    pub comparison: Comparison,
    pub continuous: bool,
    pub since: usize,
    pub budget: u64,
}

pub(super) async fn evaluate(
    state: &mut State,
    world: &World,
    request: Request<'_>,
) -> crate::Result<Outcome> {
    let Request {
        basis,
        cell,
        reference,
        expected,
        comparison,
        continuous,
        since,
        budget,
    } = request;
    state.basis = basis;
    let mut outcome = Outcome {
        witness: None,
        evidence: Vec::new(),
        reason: None,
        evaluations: 0,
    };
    let reference = state.target.as_ref().map_or(reference, |uid| uid.as_str());
    if basis == nucleus::simulation::QuantityBasis::Available
        && world.available_unit_changed(cell, reference).await?
    {
        outcome.evaluations = 1;
        outcome.reason = Some(CoverageReason::UnsupportedUnit);
        return Ok(outcome);
    }
    let current = match &state.target {
        Some(target) => world
            .observed_quantity(cell, target.as_str(), basis)
            .cloned()
            .map(|quantity| (target.clone(), quantity)),
        None => world.quantity_with_basis(cell, reference, basis).await?,
    };
    let Some((target, current)) = current else {
        outcome.evaluations = 1;
        if world.trace.iter().any(|event| matches!(event.caused_by, nucleus::simulation::Cause::Seed {}) && matches!(event.observation, Observation::LinguaInterrupted { .. } | Observation::ActionInterrupted { .. })) {
            outcome.reason = Some(CoverageReason::Stopped);
            return Ok(outcome);
        }
        outcome.witness = Some(Witness::MissingRecord {
            reference: reference.to_owned(),
        });
        return Ok(outcome);
    };
    let expected = expected.cloned().unwrap_or_else(|| Quantity {
        value: store::exact::zero(),
        unit: current.unit.clone(),
    });
    let mut samples = Vec::new();
    if continuous && state.target.is_some() {
        let mut batches = BTreeMap::new();
        let projected = basis == nucleus::simulation::QuantityBasis::Available && (world.has_loan_quantity(cell, target.as_str()) || world.trace[since..].iter().any(|event| event.cell == cell && matches!(&event.observation, Observation::LoanQuantity {record,..} if record == &target)));
        for event in &world.trace[since..] {
            if projected
                && event.cell == cell
                && let Observation::LoanQuantity {
                    record,
                    before,
                    after,
                    ..
                } = &event.observation
                && record == &target
            {
                batches.insert(
                    (true, event.sequence),
                    (event.sequence, before.clone(), after.clone()),
                );
            }
            if event.cell == cell
                && !projected
                && let Observation::CommittedQuantity {
                    record,
                    commit,
                    before,
                    after,
                    ..
                } = &event.observation
                && record == &target
            {
                if commit.is_none() {
                    outcome.reason = Some(CoverageReason::MissingCommitEvidence);
                }
                let key = commit.map_or((true, event.sequence), |commit| (false, commit as u64));
                let entry =
                    batches
                        .entry(key)
                        .or_insert((event.sequence, before.clone(), after.clone()));
                entry.0 = event.sequence;
                entry.2 = after.clone();
            }
        }
        let mut previous = state.previous.clone();
        let mut batches: Vec<_> = batches.into_values().collect();
        batches.sort_by_key(|batch| batch.0);
        for (sequence, before, after) in batches {
            if previous
                .as_ref()
                .is_some_and(|previous| !super::equal(previous, &before))
            {
                outcome.reason = Some(CoverageReason::MissingCommitEvidence);
            }
            previous = Some(after.clone());
            samples.push((Some(sequence), after));
        }
        if previous
            .as_ref()
            .is_some_and(|previous| !super::equal(previous, &current))
        {
            outcome.reason = Some(CoverageReason::MissingCommitEvidence);
        }
    }
    if samples
        .last()
        .is_none_or(|(_, value)| !super::equal(value, &current))
    {
        samples.push((None, current.clone()));
    }
    for (sequence, observed) in samples {
        if outcome.evaluations >= budget {
            outcome.reason = Some(CoverageReason::CheckBudget);
            break;
        }
        outcome.evaluations += 1;
        if observed.unit != expected.unit {
            outcome.reason = Some(CoverageReason::UnsupportedUnit);
            continue;
        }
        if !comparison.accepts(observed.value.exact_numeric_cmp(expected.value))
            && outcome.witness.is_none()
        {
            outcome.evidence.extend(sequence);
            outcome.witness = Some(Witness::Quantity {
                record: target.clone(),
                expected: expected.clone(),
                observed,
            });
        }
    }
    state.target = Some(target);
    state.previous = Some(current);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use nucleus::simulation::{CheckDefinition, Predicate};

    #[test]
    fn checks_observe_committed_batches_instead_of_intermediate_writes() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async {
                        for atomic in [true, false] {
                            let directory = tempfile::tempdir().unwrap();
                            let mut scenario = crate::fixtures::daily();
                            scenario.cells[0].seed.truncate(1);
                            scenario.checks = vec![CheckDefinition {
                                id: "nonnegative".into(),
                                predicate: Predicate::Nonnegative {
                                    cell: "a".into(),
                                    record: "stock".into(),
                                },
                                options: Default::default(),
                            }];
                            let mut world = crate::world::World::open(scenario, directory.path())
                                .await
                                .unwrap();
                            let mut checks = crate::checks::Checks::new(&world).await.unwrap();
                            checks.observe(&mut world).await.unwrap();
                            let node = &world.nodes["a"];
                            let target =
                                store::records::resolve(&node.engine().store.pool, "stock")
                                    .await
                                    .unwrap()
                                    .unwrap()
                                    .uid;
                            let news = [-11, 11]
                                .into_iter()
                                .map(|delta| {
                                    nucleus::NewFact::quantity(
                                        &target,
                                        nucleus::DecimalValue::from_mantissa(0, delta).unwrap(),
                                        nucleus::Cause::user_edit(),
                                    )
                                })
                                .collect::<Vec<_>>();
                            if atomic {
                                node.execution
                                    .scope(engine::append::append_all(
                                        &node.engine().store,
                                        news,
                                        node.execution.now(),
                                        None,
                                    ))
                                    .await
                                    .unwrap();
                            } else {
                                for fact in news {
                                    node.execution
                                        .scope(engine::append::append_one(
                                            &node.engine().store,
                                            fact,
                                            node.execution.now(),
                                            None,
                                        ))
                                        .await
                                        .unwrap();
                                }
                            }
                            world
                                .observe("a", nucleus::simulation::Cause::Timer {})
                                .await
                                .unwrap();
                            world.steps += 1;
                            checks.observe(&mut world).await.unwrap();
                            assert_eq!(checks.findings.is_empty(), atomic, "{:?}", checks.findings);
                            assert_eq!(checks.coverage[0].reason, None);
                            assert_eq!(
                                world
                                    .quantity("a", "stock")
                                    .await
                                    .unwrap()
                                    .unwrap()
                                    .1
                                    .value
                                    .to_string(),
                                "10"
                            );
                            if atomic {
                                store::sqlx::query("UPDATE record SET quantity_mantissa = 9, quantity_scale = 0 WHERE uid = ?")
                                    .bind(&target).execute(&world.nodes["a"].engine().store.pool).await.unwrap();
                                world.observe("a", nucleus::simulation::Cause::Timer {}).await.unwrap();
                                world.steps += 1;
                                checks.observe(&mut world).await.unwrap();
                                assert!(checks.findings.is_empty());
                                assert_eq!(checks.coverage[0].reason, Some(nucleus::simulation::CoverageReason::MissingCommitEvidence));
                            }
                            for node in world.nodes.values() {
                                node.engine().store.pool.close().await;
                            }
                        }
                    });
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
