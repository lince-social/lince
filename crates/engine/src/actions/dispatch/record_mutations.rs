// non-lean-yet

use super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchFuture, Engine,
    EngineError, NewFact, Utc, invalid_dispatch, is_order_like, kind_cycles,
};

impl Engine {
    pub(super) fn dispatch_record_mutations(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchFuture<'_> {
        Box::pin(async move {
            let mut outcome = ActionOutcome::default();
            match action {
                Action::Compensate { fact } => {
                    let original = store::facts::get(&self.store.pool, &fact)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(fact.clone()))?;
                    if store::transfer_effects::protected(&self.store.pool, &original.uid).await?
                        || store::transfers::occurrence_settlement_for_application_fact(
                            &self.store.pool,
                            &original.uid,
                        )
                        .await?
                        .is_some()
                        || store::transfers::occurrence_settlement_compensation_for_fact(
                            &self.store.pool,
                            &original.uid,
                        )
                        .await?
                        .is_some()
                    {
                        return Err(EngineError::Conflict {
                        code: "typed_transfer_settlement_compensation_required",
                        message: "Transfer settlement applications and their corrections cannot be changed through generic compensation"
                            .into(),
                    });
                    }
                    if store::entries::for_fact(&self.store.pool, &original.uid)
                        .await?
                        .is_some()
                    {
                        return Err(EngineError::Conflict {
                            code: "entry_void_required",
                            message: "this Fact belongs to an Entry; use void-entry".into(),
                        });
                    }
                    if !original.delta.is_zero() {
                        outcome.facts = self
                            .append(
                                NewFact {
                                    uid: None,
                                    record_uid: original.record_uid,
                                    delta: store::exact::negate(original.delta)?,
                                    at: None,
                                    actor_uid: actor,
                                    cause: Cause {
                                        kind: CauseKind::Compensation,
                                        uid: Some(original.uid),
                                    },
                                    payload: None,
                                },
                                now,
                            )
                            .await?;
                    }
                }
                Action::AssertRecord {
                    subject,
                    predicate,
                    object,
                    quantity,
                    unit,
                } => {
                    let subject_uid = self.resolve(&subject).await?;
                    let object_uid = match object {
                        Some(object) if lince_resources::reference::is_uri(&object) => Some(
                            lince_resources::address(&object)
                                .map_err(|error| EngineError::Consequence(error))?
                                .to_string(),
                        ),
                        Some(object) => Some(self.assertion_object(&subject_uid, &object).await?),
                        None => None,
                    };
                    let predicate_uid =
                        store::vocabulary::resolve(&self.store.pool, &predicate, actor.as_deref())
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(predicate))?;
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: subject_uid.clone(),
                                mutation: crate::record_change::Mutation::Assertion {
                                    predicate: predicate_uid.clone(),
                                    object: object_uid.clone(),
                                    quantity,
                                    unit,
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                    if object_uid
                        .as_deref()
                        .is_some_and(|object| !lince_resources::reference::is_uri(object))
                        && is_order_like(&self.store.pool, &predicate_uid).await?
                    {
                        for cycle in kind_cycles(&self.store.pool, &predicate_uid).await? {
                            if cycle.contains(&subject_uid)
                                || object_uid
                                    .as_ref()
                                    .is_some_and(|object| cycle.contains(object))
                            {
                                outcome.warnings.push(format!(
                                    "these {} records form a loop: {}",
                                    cycle.len(),
                                    cycle.join(" -> ")
                                ));
                            }
                        }
                    }
                }
                Action::RetractAssertion { assertion } => {
                    let row = store::assertions::get(&self.store.pool, &assertion)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(assertion.clone()))?;
                    outcome = self
                        .change_record(
                            crate::record_change::Request {
                                id: nucleus::new_uid("op"),
                                record_uid: row.subject_uid,
                                mutation: crate::record_change::Mutation::RetractAssertion {
                                    assertion,
                                },
                            },
                            actor.as_deref(),
                        )
                        .await?;
                }
                action @ Action::RefineAssertion { .. } => {
                    outcome = self
                        .edit_record_relations_as(action, actor.as_deref(), now)
                        .await?;
                }
                action @ Action::RetractRecord { .. } => {
                    outcome = self
                        .edit_record_relations_as(action, actor.as_deref(), now)
                        .await?;
                }
                action @ Action::SetIdentity { .. } => {
                    outcome = self
                        .edit_record_relations_as(action, actor.as_deref(), now)
                        .await?;
                }
                _ => return Err(invalid_dispatch()),
            }
            Ok(ControlFlow::Continue(outcome))
        })
    }
}
