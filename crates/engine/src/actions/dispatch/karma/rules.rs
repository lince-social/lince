// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, Utc, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_rules(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::PreviewKarmaReading { source } => {
                outcome.data = Some(
                    Box::pin(self.preview_karma_reading(&source, actor.as_deref(), now)).await?,
                );
            }
            Action::PreviewKarmaProposal { .. } => {
                return Err(crate::karma_preview::invalid(
                    "Preview must use the isolated Simulation service",
                ));
            }
            Action::InspectKarmaRuleHistory { rule, limit } => {
                outcome.data = Some(
                    self.inspect_karma_history(&rule, limit, actor.as_deref())
                        .await?,
                );
            }
            Action::SaveKarmaRule {
                identity,
                rule,
                expected_revision,
                fields,
                request_id,
            } => {
                outcome = Box::pin(self.save_karma_rule(
                    rule,
                    expected_revision,
                    fields,
                    identity,
                    request_id,
                    actor.as_deref(),
                    now,
                ))
                .await?;
            }
            Action::ReviseKarmaField {
                field,
                expected_revision,
                source,
                request_id,
            } => {
                outcome = Box::pin(self.revise_karma_field(
                    field,
                    expected_revision,
                    source,
                    request_id,
                    actor.as_deref(),
                    now,
                ))
                .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
