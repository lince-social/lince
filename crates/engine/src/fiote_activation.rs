use crate::{Engine, EngineError, actions::ActionOutcome};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Activation {
    pub request_id: String,
    pub value: String,
    pub actor: Option<String>,
    pub cause: serde_json::Value,
    pub state: String,
    pub thread: Option<String>,
    pub detail: String,
}

impl Engine {
    pub fn register_fiote_availability(&self, record: &str, enabled: bool) {
        self.fiote_availability
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(record.into(), enabled);
    }

    pub(crate) async fn activate_fiote(
        &self,
        target: String,
        value: String,
        request_id: String,
        cause: serde_json::Value,
        actor: Option<&str>,
    ) -> Result<ActionOutcome, EngineError> {
        if actor.is_some() {
            return Err(EngineError::Forbidden("Fiote execution requires the local interface session; a workspace or Record grant cannot delegate the host's automation authority".into()));
        }
        let target = self.resolve(&target).await?;
        self.require_permission(actor, "record:update").await?;
        self.refuse_unreadable_karma_inputs(actor, std::slice::from_ref(&target))
            .await?;
        let amount = nucleus::DecimalValue::parse_inferred(&value)
            .map_err(|e| EngineError::Consequence(e.to_string()))?;
        if request_id.is_empty()
            || request_id.len() > 512
            || request_id.chars().any(char::is_control)
        {
            return Err(EngineError::Consequence(
                "Use an activation request identity of 1–512 characters".into(),
            ));
        }
        if amount.is_zero() {
            return Ok(ActionOutcome {
                data: Some(serde_json::json!({"activated":false,"reason":"zero value"})),
                ..Default::default()
            });
        }
        if store::records::get_extension(&self.store.pool, &target, "lince.fiote")
            .await?
            .is_none()
        {
            return Err(EngineError::Consequence("Choose a configured Fiote".into()));
        }
        let value = amount.to_string();
        let enabled = self
            .fiote_availability
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&target)
            .copied()
            .unwrap_or(false);
        let mut tx = store::write_tx(&self.store.pool).await?;
        if let Some((fiote, original_value, original_actor, original_cause, state)) = store::sqlx::query_as::<_, (String, String, Option<String>, String, String)>("SELECT fiote_uid, value, actor_uid, cause_json, state FROM fiote_activation WHERE request_id = ?").bind(&request_id).fetch_optional(&mut *tx).await? {
            if fiote != target || original_value != value || original_actor.as_deref() != actor || serde_json::from_str::<serde_json::Value>(&original_cause)? != cause {
                return Err(EngineError::Consequence("Activation identity already belongs to another request".into()));
            }
            return Ok(ActionOutcome { data: Some(serde_json::json!({"request_id":request_id,"state":state,"duplicate":true})), ..Default::default() });
        }
        if !enabled {
            return Err(EngineError::Consequence(
                "This Fiote is disabled or its runtime is unavailable".into(),
            ));
        }
        let count: i64 = store::sqlx::query_scalar("SELECT count(*) FROM fiote_activation WHERE fiote_uid = ? AND state IN ('queued','waiting')").bind(&target).fetch_one(&mut *tx).await?;
        if count >= 4096 {
            return Err(EngineError::Consequence(
                "This Fiote has reached its pending activation limit".into(),
            ));
        }
        store::sqlx::query("INSERT INTO fiote_activation (request_id, fiote_uid, actor_uid, value, cause_json, state, created_at) VALUES (?, ?, ?, ?, ?, 'queued', ?)")
            .bind(&request_id).bind(&target).bind(actor).bind(&value).bind(cause.to_string()).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(ActionOutcome {
            data: Some(serde_json::json!({"request_id":request_id,"state":"queued","value":value})),
            ..Default::default()
        })
    }

    pub async fn fiote_activations(&self, record: &str) -> Result<Vec<Activation>, EngineError> {
        let rows: Vec<(String, String, Option<String>, String, String, Option<String>, String)> = store::sqlx::query_as("SELECT request_id, value, actor_uid, cause_json, state, thread_uid, detail FROM fiote_activation WHERE fiote_uid = ? ORDER BY rowid DESC LIMIT 256").bind(record).fetch_all(&self.store.pool).await?;
        rows.into_iter()
            .map(|(request_id, value, actor, cause, state, thread, detail)| {
                Ok(Activation {
                    request_id,
                    value,
                    actor,
                    cause: serde_json::from_str(&cause)?,
                    state,
                    thread,
                    detail,
                })
            })
            .collect()
    }
}
