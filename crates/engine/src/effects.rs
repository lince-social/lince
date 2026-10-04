use std::sync::Arc;

use chrono::Utc;
use nucleus::{Cause, CauseKind, NewFact};

use crate::Engine;
use crate::append::append_one;
use crate::error::EngineError;

#[derive(Debug, Clone)]
pub struct EffectOutcome {
    pub uid: String,
    pub kind: String,
    pub ok: bool,
    pub result: String,
}

pub struct DatabaseEffects {
    pub outcomes: Vec<EffectOutcome>,
    pub pending: bool,
    pub unsupported: bool,
}

impl Engine {
    pub fn start_effect_worker(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let mut changed = self.effects_changed.subscribe();
        tokio::spawn(async move {
            let recovery = self.effect_execution.lock().await;
            if let Err(error) = store::sqlx::query("UPDATE effect_queue SET status = 'uncertain', result = 'Worker stopped after claiming this effect; inspect before retrying', finished_at = ? WHERE status = 'running'")
                .bind(nucleus::execution::now().to_rfc3339()).execute(&self.store.pool).await {
                tracing::warn!(%error, "Could not recover interrupted effects");
                return;
            }
            drop(recovery);
            if let Err(error) = self.recover_commands().await { tracing::warn!(%error, "Could not recover command evaluations"); return; }
            let mut commands_changed = self.effects_changed.subscribe();
            let work = async {
                loop {
                    changed.borrow_and_update();
                    match self.run_effects(false, Some(false)).await {
                        Ok(outcomes) if !outcomes.is_empty() => {
                            tokio::task::yield_now().await;
                            continue;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(%error, "Effect worker stopped");
                            return;
                        }
                    }
                    if changed.changed().await.is_err() {
                        return;
                    }
                }
            };
            let commands = async {
                loop {
                    commands_changed.borrow_and_update();
                    match self.run_effects(false, Some(true)).await {
                        Ok(outcomes) if !outcomes.is_empty() => {
                            tokio::task::yield_now().await;
                            continue;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(%error, "Command worker stopped");
                            return;
                        }
                    }
                    if commands_changed.changed().await.is_err() {
                        return;
                    }
                }
            };
            tokio::join!(work, commands);
        })
    }

    pub async fn run_due_effects(&self) -> Result<Vec<EffectOutcome>, EngineError> {
        self.run_effects(false, None).await
    }

    pub async fn run_database_effects(&self) -> Result<DatabaseEffects, EngineError> {
        let outcomes = self.run_effects(true, None).await?;
        let pending: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM effect_queue WHERE status IN ('queued', 'running'))",
        )
        .fetch_one(&self.store.pool)
        .await?;
        let unsupported = pending
            && store::misc::due_effects(&self.store.pool)
                .await?
                .first()
                .is_none_or(|effect| !database_effect(effect) && !self.controlled_command_effect(effect));
        Ok(DatabaseEffects {
            outcomes,
            pending,
            unsupported,
        })
    }

    async fn run_effects(&self, database_only: bool, command_lane: Option<bool>) -> Result<Vec<EffectOutcome>, EngineError> {
        let guard = self.effect_execution.lock().await;
        let signer = self.signer.lock().await.clone();
        let mut out = Vec::new();
        let effects = store::misc::due_effects_for_lane(&self.store.pool, command_lane).await?;
        drop(guard);
        for effect in effects {
            let command = matches!(effect.kind.as_str(), "command" | "signal" | "saved-command");
            if command_lane.is_some_and(|lane| lane != command) { continue; }
            crate::rule_runtime::execution_checkpoint(false).await?;
            if database_only && !database_effect(&effect) && !self.controlled_command_effect(&effect) {
                break;
            }
            if !store::misc::claim_effect(&self.store.pool, &effect.uid).await? {
                continue;
            }
            let execution = match effect.payload.get("occurrence") {
                Some(occurrence) => {
                    let occurrence = serde_json::from_value(occurrence.clone()).map_err(EngineError::Json)?;
                    crate::rule_runtime::EFFECT_OCCURRENCE.scope(occurrence, self.execute_effect(&effect.uid, &effect.kind, &effect.payload)).await
                }
                None => self.execute_effect(&effect.uid, &effect.kind, &effect.payload).await,
            };
            let (ok, result) = match execution {
                Ok(result) => result,
                Err(error @ EngineError::ExecutionLimit(_)) => return Err(error),
                Err(error) => (false, error.to_string()),
            };
            let request = effect.payload["request_id"].as_str().unwrap_or(&effect.uid);
            if !ok { self.fail_command(request, &result).await?; }
            store::misc::finish_effect(&self.store.pool, &effect.uid, ok, &result).await?;
            self.effects_changed.send_modify(|revision| *revision = revision.wrapping_add(1));
            self.resume_command_query(&effect.payload).await?;
            if let Some(origin) = &effect.origin_uid {
                match append_one(
                    &self.store,
                    NewFact {
                        uid: None,
                        record_uid: origin.clone(),
                        delta: nucleus::fact::zero_delta(),
                        at: None,
                        actor_uid: None,
                        cause: Cause { kind: CauseKind::Action, uid: Some(effect.uid.clone()) },
                        payload: Some(serde_json::json!({ "effect": effect.kind, "ok": ok, "result": result }).to_string()),
                    },
                    nucleus::execution::now(),
                    signer.as_ref(),
                ).await {
                    Ok(Some(fact)) => { let _ = self.bus.send(fact); }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(effect = effect.uid, %error, "Could not attach the effect result to its Record"),
                }
            }
            self.query_changed
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            out.push(EffectOutcome {
                uid: effect.uid,
                kind: effect.kind,
                ok,
                result,
            });
        }
        if store::karma_schedules::refresh(&self.store.pool, nucleus::execution::now()).await? { self.notify_karma_deadline_change(); }
        Ok(out)
    }

    fn controlled_command_effect(&self, effect: &store::misc::EffectRow) -> bool {
        if nucleus::execution::current().is_none() { return false; }
        if !matches!(effect.kind.as_str(), "command" | "signal" | "saved-command") { return false; }
        let command = effect.payload["saved_command"].as_str().or_else(|| effect.payload["signal"].as_str()).or_else(|| effect.payload["command_snapshot"]["uid"].as_str()).unwrap_or(&effect.uid);
        self.command_responses.read().is_ok_and(|responses| responses.as_ref().is_some_and(|responses| responses.iter().any(|response| response.command == command)))
    }

    async fn execute_effect(
        &self,
        request: &str,
        kind: &str,
        payload: &serde_json::Value,
    ) -> Result<(bool, String), EngineError> {
        let actor = payload.get("actor").and_then(|value| value.as_str());
        let rule = if let Some(uid) = payload.get("rule").and_then(|value| value.as_str()) {
            let current = store::recurrence::get(&self.store.pool, uid).await?;
            let Some(mut current) = current.filter(|current| {
                !current.is_paused()
                    && Some(current.revision)
                        == payload.get("revision").and_then(|value| value.as_i64())
            }) else {
                return Ok((
                    false,
                    "Rule was paused, revised or deleted before the effect ran".into(),
                ));
            };
            if store::records::get(&self.store.pool, &current.record_uid)
                .await?
                .is_none()
            {
                return Ok((
                    false,
                    "Rule target was deleted before the effect ran".into(),
                ));
            }
            self.require_karma_execution(Some(&current.record_uid)).await?;
            self.refuse_unreadable_karma_inputs(actor, &[crate::karma_transfer_effects::target(&current).into()])
                .await?;
            current.actor_uid = actor.map(str::to_owned);
            self.authorize_karma_rule(&current, actor).await?;
            Some(current)
        } else {
            None
        };
        match kind {
            "command" | "signal" | "saved-command" => {
                if kind == "signal" {
                    let signal = payload["signal"].as_str().ok_or_else(|| EngineError::Consequence("Signal effect has no Signal".into()))?;
                    if !self.rule_dependencies().await?.records.contains_key(signal) { return Ok((false, "Signal no longer has an active reader".into())); }
                }
                self.execute_command_effect(payload["request_id"].as_str().unwrap_or(request), payload).await
            }
            "notify" => {
                let budget = store::config::attention_budget(&self.store.pool).await?;
                let midnight = nucleus::execution::now()
                    .format("%Y-%m-%dT00:00:00+00:00")
                    .to_string();
                let delivered =
                    store::misc::notifies_delivered_since(&self.store.pool, &midnight).await?;
                Ok((
                    true,
                    if delivered >= budget {
                        format!("parked:digest {payload}")
                    } else {
                        payload.to_string()
                    },
                ))
            }
            "action" => {
                let action = serde_json::from_value::<crate::actions::Action>(
                    payload.get("action").cloned().ok_or_else(|| {
                        EngineError::Consequence("Action effect has no action".into())
                    })?,
                )
                .map_err(|error| EngineError::Consequence(error.to_string()))?;
                let outcome = Box::pin(self.act(action, actor.map(str::to_string))).await?;
                Ok((true, format!("{} facts committed", outcome.facts.len())))
            }
            "query" => {
                let target = payload
                    .get("target")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let uid = self.resolve(target).await?;
                self.refuse_unreadable(actor, std::slice::from_ref(&uid)).await?;
                let rows = protein::execute_saved(&self.store, &uid, actor).await?;
                Ok((true, format!("{} rows", rows.len())))
            }
            "consequence" => {
                let rule =
                    rule.ok_or_else(|| EngineError::Consequence("Consequence has no rule".into()))?;
                self.validate_automatic_rule(
                    &rule.consequences,
                    rule.condition.as_ref(),
                    rule.actor_uid.as_deref(),
                )
                .await?;
                let consequence: nucleus::karma::Consequence =
                    serde_json::from_value(payload["consequence"].clone())
                        .map_err(|error| EngineError::Consequence(error.to_string()))?;
                let carried: Option<nucleus::DecimalValue> =
                    serde_json::from_value(payload["carried"].clone())
                        .map_err(|error| EngineError::Consequence(error.to_string()))?;
                if matches!(consequence, nucleus::karma::Consequence::ActivateFiote) {
                    let value = carried.ok_or_else(|| EngineError::Consequence("Fiote activation needs a calculated nonzero value".into()))?;
                    let request = payload["request_id"].as_str().ok_or_else(|| EngineError::Consequence("Activation has no occurrence identity".into()))?;
                    self.activate_fiote(rule.record_uid.clone(), value.to_string(), request.into(), serde_json::json!({"kind":"karma","rule":rule.uid,"revision":rule.revision,"occurrence":payload["occurrence"],"request_id":request}), actor).await?;
                    return Ok((true, "Fiote activation requested".into()));
                }
                if consequence.transfer_target().is_some() {
                    let evaluated = serde_json::from_value(payload["transfer_evaluation"].clone()).map_err(EngineError::Json)?;
                    let request = payload["request_id"].as_str().ok_or_else(|| EngineError::Consequence("Transfer effect has no request identity".into()))?;
                    let result = self.execute_transfer_effect(&rule, &consequence, evaluated, request, nucleus::execution::now()).await?;
                    return Ok((true, result));
                }
                self.execute_deferred_consequence(
                    &rule,
                    &consequence,
                    carried,
                    nucleus::execution::now(),
                )
                .await?;
                Ok((true, "Consequence committed".into()))
            }
            other => Ok((false, format!("unknown effect kind {other}"))),
        }
    }

    async fn execute_deferred_consequence(
        &self,
        rule: &store::recurrence::Recurrence,
        consequence: &nucleus::karma::Consequence,
        carried: Option<nucleus::DecimalValue>,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), EngineError> {
        use crate::actions::Action;
        use nucleus::karma::Consequence;
        let action = match consequence {
            Consequence::ShowComponent { component } => Some(Action::PresentComponent { target: rule.record_uid.clone(), component: component.clone() }),
            Consequence::SetConcept { concept } => Some(Action::SetIdentity {
                subject: rule.record_uid.clone(),
                predicate: Some(concept.clone()),
            }),
            Consequence::AddConcept { concept } => Some(Action::AssertRecord {
                subject: rule.record_uid.clone(),
                predicate: concept.clone(),
                object: None,
                quantity: None,
                unit: None,
            }),
            Consequence::RemoveConcept { concept } => Some(Action::RetractRecord {
                subject: rule.record_uid.clone(),
                predicate: concept.clone(),
                object: None,
            }),
            Consequence::SetQuantityWhere { assertion, value } => {
                let value = value
                    .or(carried)
                    .ok_or_else(|| EngineError::Consequence("Set quantity needs a value".into()))?;
                for target in
                    store::ledger::records_with_concept(&self.store.pool, assertion).await?
                {
                    Box::pin(self.act(
                        Action::SetQuantityExact {
                            target,
                            amount: value.to_string(),
                        },
                        rule.actor_uid.clone(),
                    ))
                    .await?;
                }
                None
            }
            outward => {
                self.commit_outward_consequence(rule, outward, carried.as_ref(), now)
                    .await?;
                None
            }
        };
        if let Some(action) = action {
            Box::pin(self.act(action, rule.actor_uid.clone())).await?;
        }
        Ok(())
    }
}

fn database_effect(effect: &store::misc::EffectRow) -> bool {
    use crate::actions::Action;
    use nucleus::karma::Consequence;
    match effect.kind.as_str() {
        "notify" => true,
        "action" => matches!(
            serde_json::from_value::<Action>(effect.payload["action"].clone()),
            Ok(Action::CaptureEntry { .. }
                | Action::SetQuantityExact { .. }
                | Action::SetIdentity { .. }
                | Action::AssertRecord { .. }
                | Action::RetractRecord { .. })
        ),
        "consequence" => matches!(
            serde_json::from_value::<Consequence>(effect.payload["consequence"].clone()),
            Ok(Consequence::SetConcept { .. }
                | Consequence::AddConcept { .. }
                | Consequence::RemoveConcept { .. }
                | Consequence::SetQuantityWhere { .. }
                | Consequence::SetTransferAgreement { .. }
                | Consequence::PublishTransfer { .. }
                | Consequence::ActivateTransferFulfillment { .. })
        ),
        _ => false,
    }
}
