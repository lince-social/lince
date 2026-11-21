// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, Utc,
    invalid_dispatch,
};

impl Engine {
    pub(super) async fn dispatch_signals(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::SignalEndpoints { request } => {
                outcome.data = Some(
                    self.signal_endpoint_request(request, actor.as_deref(), now)
                        .await?,
                );
            }
            Action::CreateSignal {
                slug,
                head,
                source_kind,
                source,
                schedule,
            } => {
                self.require_permission(actor.as_deref(), "organ:update")
                    .await?;
                if source_kind != "command"
                    || nucleus::parse_duration(&schedule).is_none_or(|seconds| seconds <= 0)
                {
                    return Err(EngineError::Consequence(
                        "Signal needs a command and a positive sampling period".into(),
                    ));
                }
                outcome.created = Some(
                    store::misc::create_signal(
                        &self.store.pool,
                        store::misc::NewSignal {
                            slug: &slug,
                            head: &head,
                            source_kind: &source_kind,
                            source: &source,
                            schedule: &schedule,
                        },
                    )
                    .await?,
                );
                store::sqlx::query("UPDATE signal SET actor_uid = ? WHERE record_uid = ?")
                    .bind(actor.as_deref())
                    .bind(outcome.created.as_deref())
                    .execute(&self.store.pool)
                    .await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
