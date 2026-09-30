use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use chrono::{DateTime, Utc};
use store::karma_commands;

use crate::{Engine, EngineError, actions::Action, karma_transfer_effects::EvaluatedTransfer};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::Conflict {
        code: "karma_transfer_command_invalid",
        message: message.to_string(),
    }
}

pub(crate) fn command_uid(
    request: &str,
    person: &str,
    transfer: &str,
) -> Result<String, EngineError> {
    let id = nucleus::karma::canonical_hash(
        "lince.karma-transfer-command.v1",
        &serde_json::json!({"request":request,"person":person,"transfer":transfer}),
    )
    .map_err(invalid)?;
    Ok(format!("trc:karma:{}:effect", id.as_str()))
}

impl Engine {
    pub(crate) async fn queue_transfer_karma_command(
        &self,
        rule: &store::recurrence::Recurrence,
        action: &Action,
        evaluated: &EvaluatedTransfer,
        request: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, EngineError> {
        if store::transfers::get(&self.store.pool, &evaluated.transfer)
            .await?
            .is_some()
        {
            return Ok(false);
        }
        let signer = self
            .transfer_person_signer(&evaluated.person, None)
            .await?
            .ok_or_else(|| {
                invalid("Remote automation needs the acting Person's installed signing key")
            })?;
        self.require_permission(rule.actor_uid.as_deref(), "transfer:update")
            .await?;
        let id = nucleus::karma::canonical_hash("lince.karma-transfer-command.v1", &serde_json::json!({"request":request,"person":evaluated.person,"transfer":evaluated.transfer})).map_err(invalid)?;
        let session = format!("karma:{}", id.as_str());
        let message = "effect";
        let command_uid = command_uid(request, &evaluated.person, &evaluated.transfer)?;
        if let Some(row) =
            store::transfer_delivery::remote_command(&self.store.pool, &command_uid).await?
        {
            let state = karma_commands::get(&self.store.pool, &command_uid)
                .await?
                .ok_or_else(|| invalid("Existing remote command has no automation origin"))?;
            let existing: nucleus::transfer_delivery::TransferRemoteCommandV1 =
                serde_json::from_str(&row.payload).map_err(EngineError::Json)?;
            let bytes = B64.decode(existing.action_base64).map_err(invalid)?;
            let original: Action = serde_json::from_slice(&bytes).map_err(EngineError::Json)?;
            if serde_json::to_value(&original).map_err(EngineError::Json)?
                != serde_json::to_value(action).map_err(EngineError::Json)?
                || state.origin.rule != rule.uid
                || state.origin.revision != rule.revision
                || state.cancelled
            {
                return Err(invalid(
                    "Remote command changed or was cancelled before retry",
                ));
            }
            return Ok(true);
        }
        let challenge = format!("v1:{}", id.as_str());
        let encoded = B64.encode(serde_json::to_vec(action).map_err(EngineError::Json)?);
        let signature = signer.sign_bytes(&nucleus::action_intent::signing_bytes(
            &session, &challenge, 1, message, &encoded,
        ));
        let occurrence = crate::rule_runtime::EFFECT_OCCURRENCE
            .try_with(Clone::clone)
            .map_err(|_| invalid("Remote automation has no initiating Rule occurrence"))?;
        let origin = karma_commands::Origin {
            rule: rule.uid.clone(),
            revision: rule.revision,
            request: request.into(),
            occurrence: occurrence.clone(),
            evaluation: serde_json::to_value(evaluated).map_err(EngineError::Json)?,
        };
        let outcome = self
            .queue_remote_transfer_action_with_karma(
                action,
                &evaluated.person,
                &signer.key_id,
                &session,
                &challenge,
                1,
                message,
                &encoded,
                &signature,
                now,
                Some(&origin),
            )
            .await?
            .ok_or_else(|| invalid("The authorized remote Transfer delivery is unavailable"))?;
        if let Some(execution) = nucleus::execution::current()
            && let Some(control) = execution.control()
            && let Some(cell) = execution.cell()
        {
            control.link_effect(
                cell,
                outcome
                    .created
                    .as_deref()
                    .ok_or_else(|| invalid("Remote command has no durable identity"))?,
                &occurrence,
            );
        }
        Ok(true)
    }

    pub async fn prepare_karma_transfer_command_dispatch(
        &self,
        row: &store::transfer_delivery::RemoteCommandRow,
    ) -> Result<bool, EngineError> {
        let Some(state) = karma_commands::get(&self.store.pool, &row.command_uid).await? else {
            return Ok(true);
        };
        if state.cancelled {
            return Ok(false);
        }
        if state.dispatched_at.is_some() {
            return Ok(true);
        }
        self.access_scope(true, async {
            let result = async {
                let rule = store::recurrence::get(&self.store.pool, &state.origin.rule)
                    .await?
                    .ok_or_else(|| invalid("The remote command's Rule was deleted"))?;
                if rule.is_paused() || rule.revision != state.origin.revision {
                    return Err(invalid("The remote command's Rule changed or paused"));
                }
                self.authorize_karma_rule(&rule, rule.actor_uid.as_deref())
                    .await?;
                if let Some(condition) = &rule.condition {
                    self.karma_condition_records(
                        condition.parsed().map_err(invalid)?,
                        rule.actor_uid.as_deref(),
                    )
                    .await?;
                }
                self.transfer_stage_evaluation(&rule, nucleus::execution::now())
                    .await?;
                let evaluated: EvaluatedTransfer =
                    serde_json::from_value(state.origin.evaluation).map_err(EngineError::Json)?;
                self.check_transfer_evaluation(&evaluated, rule.actor_uid.as_deref())
                    .await
            }
            .await;
            if let Err(error) = result {
                karma_commands::cancel(&self.store.pool, &row.command_uid, &error.to_string())
                    .await?;
                return Ok(false);
            }
            karma_commands::mark_dispatched(
                &self.store.pool,
                &row.command_uid,
                nucleus::execution::now(),
            )
            .await
            .map_err(EngineError::from)
        })
        .await
    }

    pub async fn refresh_karma_transfer_command_outcomes(&self) -> Result<(), EngineError> {
        self.access_scope(true, async {
            if store::karma_schedules::refresh(&self.store.pool, nucleus::execution::now()).await? {
                self.notify_karma_deadline_change();
            }
            self.query_changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            Ok(())
        })
        .await
    }
}
