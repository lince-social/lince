use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use nucleus::karma::{ConditionBinding, Consequence, Consequences, ReferenceKind, TypedUid};
use nucleus::transfer::karma::{Participant, Snapshot};
use serde::{Deserialize, Serialize};

use crate::{Engine, EngineError, actions::Action};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvaluatedTransfer {
    pub transfer: String,
    pub person: String,
    pub revision: u64,
    pub agreement: Participant,
    pub level: Option<u8>,
    pub reads: BTreeMap<String, Snapshot>,
    pub guards: BTreeMap<String, nucleus::transfer::karma::Guard>,
}

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_transfer_consequence_invalid",
        message: message.to_string(),
    }
}

pub(crate) fn target(rule: &store::recurrence::Recurrence) -> &str {
    rule.consequences
        .iter()
        .find_map(Consequence::transfer_target)
        .unwrap_or(&rule.record_uid)
}

impl Engine {
    pub(crate) async fn bind_transfer_effects(
        &self,
        effects: Vec<Consequence>,
        bindings: &mut Vec<ConditionBinding>,
        previous: &[ConditionBinding],
    ) -> Result<Consequences, EngineError> {
        bindings.retain(|binding| !binding.reading.starts_with("consequence."));
        let mut result = Vec::with_capacity(effects.len());
        for (position, mut effect) in effects.into_iter().enumerate() {
            if let Consequence::InvokeCommand { command } = &mut effect {
                let reading = format!("consequence.command.{position}");
                let old = previous.iter().find(|binding| binding.reading == reading && binding.authored == *command);
                let uid = self.resolve(old.map_or(command.as_str(), |binding| binding.target.as_str())).await?;
                bindings.push(ConditionBinding { reading, authored: command.clone(), target: TypedUid::new(ReferenceKind::Record, &uid).map_err(invalid)? });
                *command = uid;
            }
            if let Consequence::ShowComponent { component } = &mut effect {
                for (reference, authored) in component.records_mut().into_iter().enumerate() {
                    let reading = if reference == 0 {
                        format!("consequence.component.{position}")
                    } else {
                        format!("consequence.component.{position}.{reference}")
                    };
                    let old = previous.iter().find(|binding| binding.reading == reading && binding.authored == *authored);
                    let uid = self.resolve(old.map_or(authored.as_str(), |binding| binding.target.as_str())).await?;
                    bindings.push(ConditionBinding { reading, authored: authored.clone(), target: TypedUid::new(ReferenceKind::Record, &uid).map_err(invalid)? });
                    *authored = uid;
                }
            }
            if let Some((transfer, person)) = effect.transfer_references_mut() {
                for (name, authored, kind) in [
                    ("transfer", transfer, ReferenceKind::Transfer),
                    ("person", person, ReferenceKind::Person),
                ] {
                    let reading = format!("consequence.{name}.{position}");
                    let old = previous.iter().find(|binding| {
                        binding.reading == reading && binding.authored == *authored
                    });
                    let token = old.map_or(authored.as_str(), |binding| binding.target.as_str());
                    let uid = if kind == ReferenceKind::Transfer {
                        self.resolve_karma_transfer(token).await?
                    } else {
                        let uid = self.resolve(token).await?;
                        let record = store::records::get(&self.store.pool, &uid)
                            .await?
                            .ok_or_else(|| invalid("Acting Person is unavailable"))?;
                        if record.kind != "person" {
                            return Err(invalid("Choose a Person for the Transfer consequence"));
                        }
                        uid
                    };
                    if old.is_some_and(|binding| binding.target.as_str() != uid) {
                        return Err(invalid("Transfer consequence binding changed"));
                    }
                    bindings.push(ConditionBinding {
                        reading,
                        authored: authored.clone(),
                        target: TypedUid::new(kind, &uid).map_err(invalid)?,
                    });
                    *authored = uid;
                }
            }
            result.push(effect);
        }
        self.resolve_consequences(result).await
    }

    pub(crate) async fn transfer_rule_anchor(
        &self,
        effects: &Consequences,
        actor: Option<&str>,
    ) -> Result<Option<String>, EngineError> {
        let Some(transfer) = effects.iter().find_map(Consequence::transfer_target) else {
            return Ok(None);
        };
        if effects
            .iter()
            .any(|effect| effect.transfer_target() != Some(transfer))
        {
            return Err(invalid(
                "Use a separate Rule for each Transfer or Record target",
            ));
        }
        self.require_permission(actor, "transfer:update").await?;
        let mut anchor = None;
        for effect in effects {
            let person = effect
                .transfer_person()
                .ok_or_else(|| invalid("Choose an acting Person"))?;
            let acting = self
                .transfer_action_person(actor, Some(person), None)
                .await?;
            self.transfer_person_signer(&acting, None).await?;
            let state = self.cached_transfer_snapshot(transfer, actor).await?;
            if !state.participants.contains_key(&acting) {
                return Err(invalid(
                    "Transfer consequence requires an accepted visible participant",
                ));
            }
            let current_anchor = if store::transfers::get(&self.store.pool, transfer)
                .await?
                .is_some()
            {
                self.require_transfer_origin_authority(transfer).await?;
                transfer.to_owned()
            } else {
                if self
                    .readable_remote_transfer(transfer, Some(&acting))
                    .await?
                    .is_none()
                {
                    return Err(invalid("Transfer delivery is unavailable to this Person"));
                }
                acting
            };
            if anchor
                .as_ref()
                .is_some_and(|value| value != &current_anchor)
            {
                return Err(invalid("Use one acting Person per remote Transfer Rule"));
            }
            anchor = Some(current_anchor);
        }
        Ok(anchor)
    }

    pub(crate) async fn authorize_karma_rule(
        &self,
        rule: &store::recurrence::Recurrence,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        match self.transfer_rule_anchor(&rule.consequences, actor).await? {
            Some(anchor) if anchor != rule.record_uid => {
                Err(invalid("Transfer Rule storage anchor changed"))
            }
            Some(_) => Ok(()),
            None => self.authorize_rule_target(&rule.record_uid, actor).await,
        }
    }

    pub(crate) async fn prepare_transfer_effect(
        &self,
        consequence: &Consequence,
        carried: Option<DecimalValue>,
        actor: Option<&str>,
    ) -> Result<Option<EvaluatedTransfer>, EngineError> {
        let Some(transfer) = consequence.transfer_target() else {
            return Ok(None);
        };
        let person = consequence
            .transfer_person()
            .ok_or_else(|| invalid("Choose an acting Person"))?;
        let state = self.cached_transfer_snapshot(transfer, actor).await?;
        let agreement = state
            .participants
            .get(person)
            .cloned()
            .ok_or_else(|| invalid("Acting participant's agreement is unavailable"))?;
        let level = match consequence {
            Consequence::SetTransferAgreement { level, .. } => Some(
                nucleus::karma::transfer_consequence::level(level.or(carried).ok_or_else(
                    || invalid("Agreement assignment needs a Condition result or a fixed level"),
                )?)
                .map_err(invalid)?,
            ),
            _ => None,
        };
        let reads = crate::karma_transfers::READINGS
            .try_with(|readings| readings.borrow().snapshots.clone())
            .unwrap_or_else(|_| BTreeMap::from([(state.transfer.clone(), state.clone())]));
        let mut guards = crate::karma_transfers::READINGS.try_with(|readings| readings.borrow().guards.clone()).unwrap_or_default();
        guards.entry(state.transfer.clone()).or_insert_with(|| nucleus::transfer::karma::Guard::base(&state)).participants.insert(person.into(), agreement.clone());
        Ok(Some(EvaluatedTransfer {
            transfer: transfer.into(),
            person: person.into(),
            revision: state.revision,
            agreement,
            level,
            reads,
            guards,
        }))
    }

    pub(crate) async fn execute_transfer_effect(
        &self,
        rule: &store::recurrence::Recurrence,
        consequence: &Consequence,
        evaluated: EvaluatedTransfer,
        request_id: &str,
        now: DateTime<Utc>,
    ) -> Result<String, EngineError> {
        self.access_scope(true, async {
            let mut current = store::recurrence::get(&self.store.pool, &rule.uid)
                .await?
                .ok_or_else(|| invalid("Transfer Rule was deleted"))?;
            if current.is_paused() || current.revision != rule.revision {
                return Err(invalid(
                    "Transfer Rule changed or paused before its effect ran",
                ));
            }
            current.actor_uid = rule.actor_uid.clone();
            self.authorize_karma_rule(&current, current.actor_uid.as_deref())
                .await?;
            if let Some(condition) = &current.condition {
                self.karma_condition_records(condition.parsed().map_err(invalid)?, current.actor_uid.as_deref()).await?;
            }
            let completed: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_effect_outcome o JOIN effect_queue q ON q.uid = o.effect_uid WHERE q.request_id = ? AND o.status = 'done')")
                .bind(request_id).fetch_one(&self.store.pool).await?;
            if completed { return Ok("Transfer consequence already recorded".into()); }
            let command = crate::karma_transfer_commands::command_uid(request_id, &evaluated.person, &evaluated.transfer)?;
            let queued = store::transfer_delivery::remote_command(&self.store.pool, &command).await?.is_some();
            let replayed = queued || match consequence {
                Consequence::SetTransferAgreement { after_ms: None, .. } => store::sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = ?)")
                    .bind(request_id).fetch_one(&self.store.pool).await?,
                Consequence::PublishTransfer { .. } => store::transfers::revision_for_request(&self.store.pool, request_id).await?.is_some(),
                Consequence::ActivateTransferFulfillment { promise, fulfillment, .. } => {
                    let key = crate::karma_transfer_actions::fulfillment_request_id(&evaluated.transfer, &evaluated.person, promise, fulfillment)?;
                    store::transfers::occurrences_for_activation_request(&self.store.pool, &key).await?.is_some()
                }
                _ => false,
            };
            if !replayed { self.transfer_stage_evaluation(&current, now).await?; }
            for (transfer, original) in evaluated.guards.iter().filter(|_| !replayed) {
                let state = self
                    .transfer_karma_snapshot(transfer, rule.actor_uid.as_deref())
                    .await?;
                if !original.matches(&state) {
                    return Err(invalid(
                        "Transfer state changed after evaluation; the old result was refused",
                    ));
                }
            }
            if let Consequence::SetTransferAgreement { after_ms: Some(delay), .. } = consequence {
                let position = request_id.rsplit(':').next().and_then(|position| position.parse().ok()).ok_or_else(|| invalid("The stage has no effect position"))?;
                self.save_transfer_stage(rule, evaluated, *delay, position, now).await?;
                return Ok("Transfer agreement stage scheduled".into());
            }
            if matches!(consequence, Consequence::SetTransferAgreement { after_ms: None, .. })
                && evaluated.level == Some(evaluated.agreement.guard.level) {
                return Ok("Transfer already has the requested agreement level".into());
            }
            let expected_state = Some(evaluated.guards.get(&evaluated.transfer).cloned().ok_or_else(|| invalid("Transfer effect has no evaluated target guard"))?);
            let action = match consequence {
                Consequence::SetTransferAgreement { after_ms: None, .. } => {
                    Action::AssignTransferAgreementLevel {
                        transfer: evaluated.transfer.clone(),
                        expected_revision: evaluated.revision,
                        request_id: request_id.into(),
                        person: Some(evaluated.person.clone()),
                        level: evaluated
                            .level
                            .ok_or_else(|| invalid("Agreement effect has no exact level"))?,
                        expected: Some(evaluated.agreement.guard.clone()),
                        expected_state,
                    }
                }
                Consequence::PublishTransfer { .. } => Action::PublishTransfer {
                    transfer: evaluated.transfer.clone(), expected_revision: evaluated.revision,
                    request_id: request_id.into(), person: Some(evaluated.person.clone()),
                    expected: Some(evaluated.agreement.guard.clone()),
                    expected_state,
                },
                Consequence::ActivateTransferFulfillment { promise, fulfillment, .. } => Action::ActivateTransferFulfillment {
                    transfer: evaluated.transfer.clone(), promise: promise.clone(), fulfillment: fulfillment.clone(),
                    expected_revision: evaluated.revision, request_id: request_id.into(),
                    person: Some(evaluated.person.clone()), expected: Some(evaluated.agreement.guard.clone()),
                    expected_state,
                },
                _ => return Err(invalid("This Transfer operation is not connected yet")),
            };
            if self.queue_transfer_karma_command(rule, &action, &evaluated, request_id, now).await? {
                return Ok("Transfer command queued; awaiting its origin result".into());
            }
            let before = self.transfer_karma_snapshot(&evaluated.transfer, rule.actor_uid.as_deref()).await?;
            let result = Box::pin(self.act_at(action, rule.actor_uid.clone(), now)).await;
            let after = self.transfer_karma_snapshot(&evaluated.transfer, rule.actor_uid.as_deref()).await?;
            self.observe_karma_transfer_change(before, after, now, None)?;
            result?;
            Ok("Transfer consequence committed".into())
        })
        .await
    }
}
