use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use nucleus::{Cause, CauseKind, NewFact};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::Engine;
use crate::append::append_one;
use crate::error::EngineError;

const OUTPUT_LIMIT: u64 = 65_536;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct EffectOutcome {
    pub uid: String,
    pub kind: String,
    pub ok: bool,
    pub result: String,
}

impl Engine {
    pub fn start_effect_worker(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let mut changed = self.effects_changed.subscribe();
        tokio::spawn(async move {
            let recovery = self.effect_execution.lock().await;
            if let Err(error) = store::sqlx::query("UPDATE effect_queue SET status = 'uncertain', result = 'Worker stopped after claiming this effect; inspect before retrying', finished_at = ? WHERE status = 'running'")
                .bind(Utc::now().to_rfc3339()).execute(&self.store.pool).await {
                tracing::warn!(%error, "Could not recover interrupted effects");
                return;
            }
            drop(recovery);
            loop {
                changed.borrow_and_update();
                match self.run_due_effects().await {
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
        })
    }

    pub async fn run_due_effects(&self) -> Result<Vec<EffectOutcome>, EngineError> {
        let _guard = self.effect_execution.lock().await;
        let signer = self.signer.lock().await.clone();
        let mut out = Vec::new();
        for effect in store::misc::due_effects(&self.store.pool).await? {
            let claimed = store::sqlx::query(
                "UPDATE effect_queue SET status = 'running' WHERE uid = ? AND status = 'queued'",
            )
            .bind(&effect.uid)
            .execute(&self.store.pool)
            .await?;
            if claimed.rows_affected() == 0 {
                continue;
            }
            let execution = self.execute_effect(&effect.kind, &effect.payload).await;
            let (ok, result) = match execution {
                Ok(result) => result,
                Err(error) => (false, error.to_string()),
            };
            store::misc::finish_effect(&self.store.pool, &effect.uid, ok, &result).await?;
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
                    Utc::now(),
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
        Ok(out)
    }

    async fn execute_effect(
        &self,
        kind: &str,
        payload: &serde_json::Value,
    ) -> Result<(bool, String), EngineError> {
        let actor = payload.get("actor").and_then(|value| value.as_str());
        let rule = if let Some(uid) = payload.get("rule").and_then(|value| value.as_str()) {
            let current = store::recurrence::get(&self.store.pool, uid).await?;
            let Some(current) = current.filter(|current| {
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
            self.refuse_unreadable(actor, &[current.record_uid.clone()])
                .await?;
            Some(current)
        } else {
            None
        };
        match kind {
            "command" | "signal" => {
                self.require_permission(actor, "organ:update").await?;
                let cmd = payload
                    .get("command")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if kind == "signal" {
                    let signal = payload
                        .get("signal")
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| {
                            EngineError::Consequence("Signal effect has no Signal".into())
                        })?;
                    if !self.rule_dependencies().await?.records.contains_key(signal) {
                        return Ok((false, "Signal no longer has an active reader".into()));
                    }
                }
                let (ok, result) = run_shell(cmd, COMMAND_TIMEOUT).await;
                if kind == "signal" {
                    let signal = payload
                        .get("signal")
                        .and_then(|value| value.as_str())
                        .expect("validated Signal");
                    let now = Utc::now();
                    store::misc::set_signal_sampled(&self.store.pool, signal, &now.to_rfc3339())
                        .await?;
                    if ok {
                        let value = nucleus::DecimalValue::parse_inferred(result.trim()).map_err(
                            |error| {
                                EngineError::Consequence(format!(
                                    "Signal output is not a number: {error}"
                                ))
                            },
                        )?;
                        let current = store::facts::level(&self.store.pool, signal).await?;
                        let delta = store::exact::difference(value, current)?;
                        self.append(NewFact::quantity(signal, delta, Cause::signal(signal)), now)
                            .await?;
                    }
                }
                Ok((ok, result))
            }
            "notify" => {
                let budget = store::config::attention_budget(&self.store.pool).await?;
                let midnight = Utc::now().format("%Y-%m-%dT00:00:00+00:00").to_string();
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
                let rows = protein::execute_saved(&self.store, target, None).await?;
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
                self.execute_deferred_consequence(&rule, &consequence, carried, Utc::now())
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

async fn read_output(reader: impl AsyncRead + Unpin) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err("Command output exceeded 64 KiB per stream".into());
    }
    Ok(bytes)
}

async fn run_shell(cmd: &str, timeout: Duration) -> (bool, String) {
    if cmd.trim().is_empty() {
        return (false, "empty command".into());
    }
    let mut command = tokio::process::Command::new("sh");
    command
        .arg("-c")
        .arg(cmd)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(target_os = "linux")]
    {
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return (false, error.to_string()),
    };
    #[cfg(target_os = "linux")]
    let _group = CommandGroup(
        child
            .id()
            .and_then(|pid| rustix::process::Pid::from_raw(pid as i32)),
    );
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let result = tokio::time::timeout(timeout, async {
        tokio::try_join!(read_output(stdout), read_output(stderr), async {
            child.wait().await.map_err(|error| error.to_string())
        })
    })
    .await;
    match result {
        Ok(Ok((stdout, stderr, status))) => {
            let mut text = String::from_utf8_lossy(&stdout).trim().to_string();
            if !status.success() {
                text.push_str(&String::from_utf8_lossy(&stderr));
            }
            (status.success(), text)
        }
        Ok(Err(error)) => {
            let _ = child.kill().await;
            (false, error)
        }
        Err(_) => {
            let _ = child.kill().await;
            (
                false,
                format!("Command exceeded {} ms", timeout.as_millis()),
            )
        }
    }
}

#[cfg(target_os = "linux")]
struct CommandGroup(Option<rustix::process::Pid>);

#[cfg(target_os = "linux")]
impl Drop for CommandGroup {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn commands_are_bounded_and_report_failure() {
        assert_eq!(
            run_shell("printf '42'", COMMAND_TIMEOUT).await,
            (true, "42".into())
        );
        assert!(!run_shell("exit 7", COMMAND_TIMEOUT).await.0);
        assert!(
            run_shell("sleep 30", Duration::from_millis(20))
                .await
                .1
                .contains("exceeded")
        );
        assert!(
            run_shell("yes x", COMMAND_TIMEOUT)
                .await
                .1
                .contains("64 KiB")
        );
    }
}
