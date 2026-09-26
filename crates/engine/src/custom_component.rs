use crate::{Engine, EngineError, actions::ActionOutcome};
use chrono::{DateTime, Utc};
use nucleus::{Cause, NewFact, RecordKind};

pub const FORMAT: &str = "lince.custom_component";
pub const MAX_BYTES: usize = 256 * 1024;

impl Engine {
    pub(crate) async fn create_custom_component(
        &self,
        head: String,
        body: String,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if head.trim().is_empty()
            || head.chars().count() > 80
            || head.chars().any(char::is_control)
            || body.len() > MAX_BYTES
        {
            return Err(EngineError::Consequence(
                "Invalid custom component name or size".into(),
            ));
        }
        let value: serde_json::Value = serde_json::from_str(&body)
            .map_err(|_| EngineError::Consequence("Invalid custom component".into()))?;
        if value["format"] != FORMAT
            || value["castle"]["name"] != head
            || !value["castle"]["parts"].as_array().is_some_and(|parts| {
                !parts.is_empty()
                    && parts.len() <= 256
                    && parts.iter().all(serde_json::Value::is_object)
            })
        {
            return Err(EngineError::Consequence("Invalid custom component".into()));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("No local Organ".into()))?;
        let uid = nucleus::new_uid("r");
        let mut tx = store::write_tx(&self.store.pool).await?;
        store::records::create_with_uid_on(
            &mut tx,
            &uid,
            store::records::NewRecord {
                slug: None,
                kind: RecordKind::Sand,
                head: &head,
                body: &body,
                quantity: store::exact::zero(),
            },
            &organ.uid,
            None,
        )
        .await?;
        store::sqlx::query("INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level) VALUES (?, 'organ', ?, ?, 'visible')")
            .bind(nucleus::new_uid("v")).bind(&organ.uid).bind(&uid).execute(&mut *tx).await?;
        tx.commit().await?;
        let facts = self
            .append(
                NewFact {
                    actor_uid: actor,
                    ..NewFact::quantity_f64(uid.clone(), 1.0, Cause::user_edit())
                },
                now,
            )
            .await?;
        Ok(ActionOutcome {
            created: Some(uid),
            facts,
            ..Default::default()
        })
    }
}
