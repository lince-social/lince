use chrono::{DateTime, TimeDelta, Utc};
use nucleus::{Cause, Fact, NewFact};

use crate::Engine;
use crate::error::EngineError;

impl Engine {
    pub async fn sample_due_signals(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let mut committed = Vec::new();
        for signal in store::misc::list_signals(&self.store.pool).await? {
            let Some(interval) = nucleus::parse_duration(&signal.schedule) else {
                continue;
            };
            let due = match &signal.last_sampled_at {
                None => true,
                Some(last) => DateTime::parse_from_rfc3339(last)
                    .map(|last| last.with_timezone(&Utc) + TimeDelta::seconds(interval) <= now)
                    .unwrap_or(true),
            };
            if !due {
                continue;
            }
            let sampled = match signal.source_kind.as_str() {
                "command" => sample_command(&signal.source).await,
                _ => None,
            };
            store::misc::set_signal_sampled(
                &self.store.pool,
                &signal.record_uid,
                &now.to_rfc3339(),
            )
            .await?;
            let Some(value) = sampled else { continue };
            let delta = value - signal.current_value;
            if delta == 0.0 {
                continue;
            }
            let facts = self
                .append(
                    NewFact::quantity_f64(
                        signal.record_uid.clone(),
                        delta,
                        Cause::signal(signal.record_uid.clone()),
                    ),
                    now,
                )
                .await?;
            committed.extend(facts);
        }
        Ok(committed)
    }
}

async fn sample_command(cmd: &str) -> Option<f64> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}
