use crate::{Engine, actions::ActionOutcome, error::EngineError};
use chrono::{DateTime, Utc};

impl Engine {
    pub(crate) async fn create_tagged_record(
        &self,
        head: String,
        body: String,
        quantity: f64,
        tags: Vec<String>,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        if head.len() > 500 || body.len() > 40000 || !quantity.is_finite() || tags.len() > 40 {
            return Err(EngineError::Consequence(
                "Use at most 500 bytes for the title, 40000 for the description, and 40 tags."
                    .into(),
            ));
        }
        let assertions = tags
            .into_iter()
            .map(|predicate| crate::record_creation::Assertion {
                predicate,
                object: None,
                quantity: None,
                unit: None,
            })
            .collect();
        self.create_record_draft(
            crate::record_creation::Draft {
                head: head.trim().into(),
                body,
                quantity: store::exact::from_f64(quantity).to_string(),
                assertions,
                ..Default::default()
            },
            actor,
            now,
        )
        .await
    }
}
