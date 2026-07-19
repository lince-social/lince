//! Effect runner (blueprint VI): shell/notify/action/query effects run OUTSIDE
//! evaluation, from a durable queue, results logged back as zero-delta
//! provenance facts on the originating rule record.
//!
//! `action` effects re-enter the one write surface (`Engine::act`) with the
//! typed Action carried in the payload; `query` effects execute a saved
//! Protein by slug/uid and log the row count — the read stays read-only.

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

impl Engine {
    pub async fn run_due_effects(&self) -> Result<Vec<EffectOutcome>, EngineError> {
        let signer = self.signer.lock().await.clone();
        let mut out = Vec::new();
        for effect in store::misc::due_effects(&self.store.pool).await? {
            let (ok, result) = match effect.kind.as_str() {
                "command" => {
                    let cmd = effect
                        .payload
                        .get("command")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    run_shell(&cmd).await
                }
                // Notify (XIII.2): the queue row is the deliverable; platform
                // channels render it. The hard daily budget parks overflow in
                // the digest instead of interrupting.
                "notify" => {
                    let budget = store::config::attention_budget(&self.store.pool).await?;
                    let midnight = Utc::now().format("%Y-%m-%dT00:00:00+00:00").to_string();
                    let delivered =
                        store::misc::notifies_delivered_since(&self.store.pool, &midnight).await?;
                    if delivered >= budget {
                        (true, format!("parked:digest {}", effect.payload))
                    } else {
                        (true, effect.payload.to_string())
                    }
                }
                "action" => match effect
                    .payload
                    .get("action")
                    .cloned()
                    .map(serde_json::from_value::<crate::actions::Action>)
                {
                    Some(Ok(action)) => match self.act(action, None).await {
                        Ok(outcome) => (true, format!("{} facts committed", outcome.facts.len())),
                        Err(e) => (false, e.to_string()),
                    },
                    Some(Err(e)) => (false, format!("bad action payload: {e}")),
                    None => (false, "action effect without an action".into()),
                },
                "query" => {
                    let target = effect
                        .payload
                        .get("target")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    match protein::execute_saved(&self.store, target, None).await {
                        Ok(rows) => (true, format!("{} rows", rows.len())),
                        Err(e) => (false, e.to_string()),
                    }
                }
                other => (false, format!("unknown effect kind {other}")),
            };
            store::misc::finish_effect(&self.store.pool, &effect.uid, ok, &result).await?;
            // provenance: a zero-delta fact on the originating rule record
            if let Some(origin) = &effect.origin_uid {
                let _ = append_one(
                    &self.store,
                    NewFact {
                        uid: None,
                        record_uid: origin.clone(),
                        delta: 0.0,
                        at: None,
                        actor_uid: None,
                        cause: Cause {
                            kind: CauseKind::Action,
                            uid: Some(effect.uid.clone()),
                        },
                        payload: Some(
                            serde_json::json!({
                                "effect": effect.kind, "ok": ok, "result": result.clone(),
                            })
                            .to_string(),
                        ),
                    },
                    Utc::now(),
                    signer.as_ref(),
                )
                .await;
            }
            out.push(EffectOutcome {
                uid: effect.uid,
                kind: effect.kind,
                ok,
                result,
            });
        }
        Ok(out)
    }
}

async fn run_shell(cmd: &str) -> (bool, String) {
    if cmd.trim().is_empty() {
        return (false, "empty command".into());
    }
    match tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .await
    {
        Ok(output) => {
            let ok = output.status.success();
            let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ok {
                text.push_str(&String::from_utf8_lossy(&output.stderr));
            }
            (ok, text)
        }
        Err(e) => (false, e.to_string()),
    }
}
