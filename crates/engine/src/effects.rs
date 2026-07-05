//! Effect runner (blueprint VI): shell/notify actions run OUTSIDE evaluation,
//! from a durable queue, results logged back as zero-delta provenance facts
//! on the originating rule record.

use chrono::Utc;
use nucleus::{Cause, CauseKind, NewFact};
use store::Store;

use crate::append::append_one;
use crate::error::EngineError;

#[derive(Debug, Clone)]
pub struct EffectOutcome {
    pub uid: String,
    pub kind: String,
    pub ok: bool,
    pub result: String,
}

pub async fn run_due(
    store: &Store,
    signer: Option<&crate::trust::Signer>,
) -> Result<Vec<EffectOutcome>, EngineError> {
    let mut out = Vec::new();
    for effect in store::misc::due_effects(&store.pool).await? {
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
            // Notify delivery per platform lands with Attention (XIII); for now the
            // queue row itself is the deliverable and the digest reads it.
            "notify" => (true, effect.payload.to_string()),
            other => (false, format!("unknown effect kind {other}")),
        };
        store::misc::finish_effect(&store.pool, &effect.uid, ok, &result).await?;
        // provenance: a zero-delta fact on the originating rule record
        if let Some(origin) = &effect.origin_uid {
            let _ = append_one(
                store,
                NewFact {
                    uid: None,
                    record_uid: origin.clone(),
                    delta: 0.0,
                    at: None,
                    actor_uid: None,
                    cause: Cause { kind: CauseKind::Action, uid: Some(effect.uid.clone()) },
                    payload: Some(
                        serde_json::json!({ "effect": effect.kind, "ok": ok }).to_string(),
                    ),
                },
                Utc::now(),
                signer,
            )
            .await;
        }
        out.push(EffectOutcome { uid: effect.uid, kind: effect.kind, ok, result });
    }
    Ok(out)
}

async fn run_shell(cmd: &str) -> (bool, String) {
    if cmd.trim().is_empty() {
        return (false, "empty command".into());
    }
    match tokio::process::Command::new("sh").arg("-c").arg(cmd).output().await {
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
