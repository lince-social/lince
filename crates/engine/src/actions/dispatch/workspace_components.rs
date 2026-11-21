// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    invalid_dispatch,
};

impl Engine {
    pub(super) async fn dispatch_workspace_components(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::SandPackage { request } => {
                outcome = self.sand_package_command(request, actor.as_deref()).await?;
            }
            Action::CreateCustomComponent { head, body } => {
                outcome = self.create_custom_component(head, body, actor, now).await?;
            }
            Action::PresentComponent { target, component } => {
                outcome = self
                    .present_component(target, component, actor.as_deref())
                    .await?;
            }
            Action::PreviewAreaTransition {
                target,
                changes,
                constraints,
            } => {
                outcome.data = Some(
                    serde_json::to_value(
                        self.preview_area_transition(target, changes, constraints)
                            .await?,
                    )
                    .map_err(EngineError::Json)?,
                );
            }
            Action::ApplyAreaTransition {
                request_id,
                preview,
            } => {
                outcome = self
                    .apply_area_transition(request_id, preview, actor, now)
                    .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
