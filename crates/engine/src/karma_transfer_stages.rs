use chrono::{DateTime, Utc};
use nucleus::karma::scheduled_change::{BoundaryInput, DateInput, Purpose};
use nucleus::karma::{Consequence, DurationMs};

use crate::{Engine, EngineError, karma_transfer_effects::EvaluatedTransfer};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_transfer_stage_changed",
        message: message.to_string(),
    }
}

impl Engine {
    pub(crate) async fn check_transfer_evaluation(
        &self,
        evaluated: &EvaluatedTransfer,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        for (transfer, original) in &evaluated.guards {
            let current = self.transfer_karma_snapshot(transfer, actor).await?;
            if !original.matches(&current) {
                return Err(invalid(
                    "Transfer state changed after evaluation; the old result was refused",
                ));
            }
        }
        Ok(())
    }

    pub(crate) async fn transfer_stage_evaluation(
        &self,
        rule: &store::recurrence::Recurrence,
        now: DateTime<Utc>,
    ) -> Result<Option<EvaluatedTransfer>, EngineError> {
        let Some(origin) = store::karma_stages::for_rule(&self.store.pool, &rule.uid).await? else {
            return Ok(None);
        };
        let evaluated = serde_json::from_value(origin.evaluation).map_err(EngineError::Json)?;
        let result = async {
            let parent = store::recurrence::get(&self.store.pool, &origin.parent_rule)
                .await?
                .ok_or_else(|| invalid("The stage's parent Rule was deleted"))?;
            if parent.is_paused() || parent.revision != origin.parent_revision {
                return Err(invalid("The stage's parent Rule changed or paused"));
            }
            self.authorize_karma_rule(&parent, parent.actor_uid.as_deref())
                .await?;
            self.check_transfer_evaluation(&evaluated, parent.actor_uid.as_deref())
                .await
        }
        .await;
        if let Err(error) = result {
            store::karma_stages::cancel(&self.store.pool, &rule.uid, &error.to_string(), now)
                .await?;
            self.notify_karma_deadline_change();
            return Err(error);
        }
        Ok(Some(evaluated))
    }

    pub(crate) async fn save_transfer_stage(
        &self,
        rule: &store::recurrence::Recurrence,
        evaluated: EvaluatedTransfer,
        delay: DurationMs,
        position: usize,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        let changed = evaluated.agreement.changed_at_ms.ok_or_else(|| {
            invalid("A relative stage needs an actual preceding agreement change")
        })?;
        let change_uid = evaluated
            .agreement
            .guard
            .change_uid
            .clone()
            .ok_or_else(|| invalid("The preceding agreement change has no stable identity"))?;
        let due_at_ms = changed
            .checked_add(delay.get())
            .ok_or_else(|| invalid("The stage's due date is out of range"))?;
        let occurrence = crate::rule_runtime::EFFECT_OCCURRENCE
            .try_with(Clone::clone)
            .map_err(|_| invalid("The stage has no initiating Rule occurrence"))?;
        let origin = store::karma_stages::Origin {
            parent_rule: rule.uid.clone(),
            parent_revision: rule.revision,
            position,
            change_uid,
            due_at_ms,
            evaluation: serde_json::to_value(&evaluated).map_err(EngineError::Json)?,
            occurrence,
        };
        if store::karma_stages::existing(&self.store.pool, &origin)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let request = nucleus::karma::canonical_hash("lince.karma-transfer-stage.v1", &serde_json::json!({"rule":rule.uid,"revision":rule.revision,"position":position,"change":origin.change_uid})).map_err(invalid)?;
        let consequence = Consequence::SetTransferAgreement {
            transfer: evaluated.transfer.clone(),
            person: evaluated.person.clone(),
            level: Some(store::exact::integer(i128::from(
                evaluated
                    .level
                    .ok_or_else(|| invalid("The stage has no exact target level"))?,
            ))),
            after_ms: None,
        };
        Box::pin(self.save_karma_schedule_with_stage(
            None,
            None,
            format!("Agreement stage for {}", evaluated.transfer),
            vec![BoundaryInput {
                purpose: Purpose::Once,
                date: DateInput::Instant {
                    at_ms: due_at_ms.max(now.timestamp_millis()),
                },
                target: evaluated.transfer,
                consequences: vec![consequence],
            }],
            format!("stage:{}", request.as_str()),
            rule.actor_uid.as_deref(),
            now,
            Some(&origin),
        ))
        .await?;
        self.notify_karma_deadline_change();
        Ok(())
    }
}
