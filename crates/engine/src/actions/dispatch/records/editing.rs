// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    NewFact, Utc, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_records_editing(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::TransitionRecord {
                subject,
                retract,
                assert,
                quantity,
            } => {
                let subject_uid = self.resolve(&subject).await?;
                self.reject_direct_transfer_record_mutation(&subject_uid)
                    .await?;
                let mut retract_uids = Vec::new();
                for concept in retract {
                    let uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    if !retract_uids.contains(&uid) {
                        retract_uids.push(uid);
                    }
                }
                let mut assert_uids = Vec::new();
                for concept in assert {
                    let uid = store::concepts::resolve(&self.store.pool, &concept)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(concept))?;
                    if !assert_uids.contains(&uid) {
                        assert_uids.push(uid);
                    }
                }
                retract_uids.retain(|uid| !assert_uids.contains(uid));

                let signer = self.signer.lock().await.clone();
                let mut tx = store::write_tx(&self.store.pool).await?;
                let checkpoint = self
                    .record_checkpoint_on(
                        &mut tx,
                        actor.as_deref(),
                        vec![subject_uid.clone()],
                        protein::authority::Operation::Update,
                    )
                    .await?;
                store::assertions::transition_unary(
                    &mut tx,
                    &subject_uid,
                    &retract_uids,
                    &assert_uids,
                    actor.as_deref(),
                )
                .await?;
                let fact = if let Some(value) = quantity {
                    let current = store::records::quantity_in_transaction(&mut tx, &subject_uid)
                        .await?
                        .ok_or_else(|| EngineError::UnknownRecord(subject_uid.clone()))?;
                    let target = store::exact::from_f64(value);
                    if target == current {
                        None
                    } else {
                        crate::append::append_one_in_transaction(
                            &mut tx,
                            NewFact::quantity(
                                subject_uid.clone(),
                                store::exact::difference(target, current)?,
                                Cause::user_edit(),
                            ),
                            now,
                            signer.as_ref(),
                        )
                        .await?
                    }
                } else {
                    None
                };
                if let Some(checkpoint) = checkpoint {
                    checkpoint.finish(&mut tx, Default::default()).await?;
                }
                tx.commit().await?;
                if let Some(fact) = fact {
                    outcome.facts = self.observe_committed_fact(fact, now).await?;
                }
            }
            Action::Activate { target } => {
                return Box::pin(self.act(Action::SetQuantity { target, value: 1.0 }, actor))
                    .await
                    .map(ControlFlow::Break);
            }
            Action::Deactivate { target } => {
                return Box::pin(self.act(Action::SetQuantity { target, value: 0.0 }, actor))
                    .await
                    .map(ControlFlow::Break);
            }
            Action::DeleteRecord { target } => {
                let uid = self.resolve(&target).await?;
                self.reject_direct_transfer_record_mutation(&uid).await?;
                self.check_delete_permission(&uid, actor.as_deref()).await?;
                if store::karma::frequencies::get_handle(&self.store.pool, &uid)
                    .await?
                    .is_some()
                {
                    store::frequency::delete(&self.store.pool, &uid)
                        .await
                        .map_err(|error| EngineError::Conflict {
                            code: "frequency_in_use",
                            message: error.to_string(),
                        })?;
                    return Ok(ControlFlow::Break(outcome));
                }
                outcome.facts = self
                    .change_record_scalar_as(
                        &uid,
                        crate::record_policy::ScalarChange::Delete,
                        actor.as_deref(),
                        now,
                    )
                    .await?;
            }
            Action::EditRecordText { target, head, body } => {
                let uid = self.resolve(&target).await?;
                outcome.facts = self
                    .write_record_text_as(&uid, head.as_deref(), body.as_deref(), actor.as_deref())
                    .await?;
            }
            Action::SetSlug { target, slug } => {
                outcome = self
                    .change_record(
                        crate::record_change::Request {
                            id: nucleus::new_uid("op"),
                            record_uid: self.resolve(&target).await?,
                            mutation: crate::record_change::Mutation::Slug {
                                value: slug.filter(|value| !value.is_empty()),
                            },
                        },
                        actor.as_deref(),
                    )
                    .await?;
            }
            Action::SetUnit { target, unit } => {
                let uid = self.resolve(&target).await?;
                let unit_uid = self.resolve_concept_opt(unit).await?;
                outcome.facts = self
                    .change_record_scalar_as(
                        &uid,
                        crate::record_policy::ScalarChange::Unit(unit_uid.as_deref()),
                        actor.as_deref(),
                        now,
                    )
                    .await?;
            }
            action @ Action::SetAssertionOrder { .. } => {
                outcome = self
                    .edit_record_relations_as(action, actor.as_deref(), now)
                    .await?;
            }
            action @ Action::SetPlace { .. } => {
                outcome = self
                    .edit_record_relations_as(action, actor.as_deref(), now)
                    .await?;
            }
            Action::GrantVisibility {
                subject_kind,
                subject,
                target,
            } => {
                outcome = self
                    .grant_record_visibility_as(
                        &subject_kind,
                        subject.as_deref(),
                        &target,
                        actor.as_deref(),
                        now,
                    )
                    .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
