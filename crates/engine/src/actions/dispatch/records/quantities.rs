// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    NewFact, Utc, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_records_quantities(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::SetQuantityExact { target, amount } => {
                outcome = self
                    .change_record(
                        crate::record_change::Request {
                            id: nucleus::new_uid("op"),
                            record_uid: self.resolve(&target).await?,
                            mutation: crate::record_change::Mutation::Quantity { value: amount },
                        },
                        actor.as_deref(),
                    )
                    .await?;
            }
            Action::SetQuantity { target, value } => {
                if !value.is_finite() {
                    return Err(EngineError::Consequence("Quantity must be finite".into()));
                }
                outcome = self
                    .change_record(
                        crate::record_change::Request {
                            id: nucleus::new_uid("op"),
                            record_uid: self.resolve(&target).await?,
                            mutation: crate::record_change::Mutation::Quantity {
                                value: store::exact::from_f64(value).to_string(),
                            },
                        },
                        actor.as_deref(),
                    )
                    .await?;
            }
            Action::AddQuantityGroupExact { changes } => {
                if changes.is_empty() || changes.len() > 32 {
                    return Err(EngineError::Consequence(
                        "choose between one and 32 Record changes".into(),
                    ));
                }
                let mut records = std::collections::BTreeSet::new();
                let mut facts = Vec::new();
                for (target, delta) in changes {
                    let uid = self.resolve(&target).await?;
                    self.reject_direct_transfer_record_mutation(&uid).await?;
                    if !records.insert(uid.clone()) {
                        return Err(EngineError::Consequence(
                            "a Record may appear only once in a group".into(),
                        ));
                    }
                    facts.push(NewFact {
                        uid: None,
                        record_uid: uid,
                        delta,
                        at: None,
                        actor_uid: actor.clone(),
                        cause: Cause::user_edit(),
                        payload: None,
                    });
                }
                outcome.facts = self
                    .append_record_changes_as(facts, actor.as_deref(), now)
                    .await?;
            }
            Action::AddQuantityExact { target, delta } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                if delta.mantissa() != 0 {
                    outcome.facts = self
                        .append_record_changes_as(
                            vec![NewFact {
                                uid: None,
                                record_uid: uid,
                                delta,
                                at: None,
                                actor_uid: actor.clone(),
                                cause: Cause::user_edit(),
                                payload: None,
                            }],
                            actor.as_deref(),
                            now,
                        )
                        .await?;
                }
            }
            Action::AddQuantity { target, delta } => {
                if !delta.is_finite() {
                    return Err(EngineError::Consequence("Quantity must be finite".into()));
                }
                return Box::pin(self.act(
                    Action::AddQuantityExact {
                        target,
                        delta: store::exact::from_f64(delta),
                    },
                    actor,
                ))
                .await
                .map(ControlFlow::Break);
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
