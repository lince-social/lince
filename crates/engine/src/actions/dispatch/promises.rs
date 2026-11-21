// non-lean-yet

use super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchOutcome, Engine,
    EngineError, NewFact, PromiseState, Utc, invalid_dispatch,
};

impl Engine {
    pub(super) async fn dispatch_promises(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreatePromise {
                record,
                delta,
                window_end,
                party,
                open,
            } => {
                let record_uid = self.resolve(&record).await?;
                let state = if open {
                    PromiseState::Open
                } else {
                    PromiseState::Proposed
                };
                let party = if open {
                    Some(
                        self.transfer_action_person(actor.as_deref(), party.as_deref(), None)
                            .await?,
                    )
                } else {
                    match party {
                        Some(token) => Some(self.resolve(token.trim()).await?),
                        None => None,
                    }
                };
                outcome.created = Some(
                    store::misc::insert_promise(
                        &self.store.pool,
                        store::misc::NewPromise {
                            record_uid: Some(record_uid),
                            delta,
                            window_end,
                            party_uid: party,
                            state: Some(state),
                            ..Default::default()
                        },
                    )
                    .await?,
                );
            }
            Action::PromiseTransition { promise, to } => {
                let row = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                if row.transfer_uid.is_some() {
                    return Err(EngineError::Conflict {
                        code: "transfer_phase_3_not_available",
                        message:
                            "bundled promise state follows the revision-bound Transfer workflow"
                                .into(),
                    });
                }
                let next = PromiseState::transition(row.state, to)?;
                store::misc::set_promise_state(&self.store.pool, &promise, next).await?;
                if let Some(record_uid) = row.record_uid {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid,
                                delta: nucleus::fact::zero_delta(),
                                at: None,
                                actor_uid: actor,
                                cause: Cause { kind: CauseKind::Action, uid: Some(promise) },
                                payload: Some(
                                    serde_json::json!({
                                        "promise": { "from": row.state.as_str(), "to": next.as_str() }
                                    })
                                    .to_string(),
                                ),
                            },
                            now,
                        )
                        .await?;
                }
            }
            Action::EditPromiseDelta { promise, delta } => {
                if !delta.is_finite() || delta == 0.0 {
                    return Err(EngineError::Consequence(
                        "promise delta must be finite and non-zero".into(),
                    ));
                }
                let row = store::misc::get_promise(&self.store.pool, &promise)
                    .await?
                    .ok_or_else(|| EngineError::UnknownRecord(promise.clone()))?;
                if let Some(transfer_uid) = row.transfer_uid.as_deref() {
                    self.require_transfer_editor(transfer_uid, actor.as_deref())
                        .await?;
                    return Err(EngineError::Conflict {
                        code: "transfer_revision_required",
                        message: "bundled promises must use revise-transfer-promise with expected_revision and request_id".into(),
                    });
                }
                store::misc::set_promise_delta(&self.store.pool, &promise, delta).await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
