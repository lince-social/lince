// non-lean-yet

use super::{Action, ControlFlow, DateTime, DispatchOutcome, Engine, Utc, invalid_dispatch};

impl Engine {
    pub(super) async fn dispatch_social(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let outcome;
        match action {
            Action::Social { request } => {
                outcome = self.social_command(request, actor.as_deref(), now).await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
