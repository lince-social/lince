// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, CauseKind, ControlFlow, DateTime, DispatchOutcome, Engine,
    EngineError, NewFact, Utc, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_karma_commands(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::Decide { decision, answer } => {
                store::misc::answer_decision(&self.store.pool, &decision, &answer).await?;
                let chosen_action = store::misc::list_decisions(&self.store.pool)
                    .await?
                    .into_iter()
                    .find(|d| d.record_uid == decision)
                    .and_then(|d| {
                        d.options.as_array()?.iter().find_map(|option| {
                            (option.get("label")?.as_str()? == answer)
                                .then(|| option.get("action").cloned())
                                .flatten()
                        })
                    });
                let current = store::records::quantity(&self.store.pool, &decision)
                    .await?
                    .unwrap_or_else(store::exact::zero);
                if !current.is_zero() {
                    outcome.facts = self
                        .append(
                            NewFact {
                                uid: None,
                                record_uid: decision,
                                delta: store::exact::negate(current)?,
                                at: None,
                                actor_uid: actor.clone(),
                                cause: Cause {
                                    kind: CauseKind::Action,
                                    uid: None,
                                },
                                payload: Some(serde_json::json!({ "answer": answer }).to_string()),
                            },
                            now,
                        )
                        .await?;
                }
                if let Some(action_json) = chosen_action {
                    let action: Action = serde_json::from_value(action_json)
                        .map_err(|e| EngineError::Consequence(format!("bad option action: {e}")))?;
                    let inner = Box::pin(self.act_at(action, actor, now)).await?;
                    outcome.facts.extend(inner.facts);
                    outcome.warnings.extend(inner.warnings);
                    if outcome.created.is_none() {
                        outcome.created = inner.created;
                    }
                }
            }
            Action::SaveKarmaCommand {
                target,
                expected_revision,
                slug,
                head,
                configuration,
                host,
                unlimited,
            } => {
                outcome.created = Some(
                    self.save_command(
                        target,
                        expected_revision,
                        slug,
                        head,
                        configuration,
                        host,
                        unlimited,
                        actor.as_deref(),
                    )
                    .await?,
                );
            }
            Action::RunKarmaCommand {
                command,
                request_id,
                numeric,
            } => {
                self.run_saved_command(&command, &request_id, numeric, actor.as_deref())
                    .await?;
                outcome.data = Some(serde_json::json!({"request":request_id,"status":"queued"}));
            }
            Action::InspectKarmaCommands => {
                outcome.data = Some(self.inspect_commands(actor.as_deref()).await?);
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
