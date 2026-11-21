// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_habits(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::PreviewKarmaHabit { input } => {
                outcome.data = Some(
                    serde_json::to_value(
                        self.preview_karma_habit(input, actor.as_deref(), now)
                            .await?,
                    )
                    .map_err(EngineError::Json)?,
                );
            }
            Action::ImportKarmaHabit {
                input,
                expected_preview,
                request_id,
            } => {
                let imported = Box::pin(self.import_karma_habit(
                    input,
                    expected_preview,
                    request_id,
                    actor.as_deref(),
                    now,
                ))
                .await?;
                outcome.created = imported
                    .objects
                    .iter()
                    .find(|object| object.kind == crate::karma_habits::Kind::Record)
                    .map(|object| object.uid.clone());
                outcome.data = Some(serde_json::to_value(imported).map_err(EngineError::Json)?);
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
