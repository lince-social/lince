// non-lean-yet

use super::{
    Action, ActionOutcome, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError, RecordKind,
    Utc, invalid_dispatch,
};

impl Engine {
    pub(super) async fn dispatch_fiote(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::ConfigureFiote {
                target,
                prompt_parent,
                run_assigned,
            } => {
                let uid = self.resolve(&target).await?;
                self.configure_fiote(
                    &uid,
                    prompt_parent.as_deref(),
                    run_assigned,
                    actor.as_deref(),
                )
                .await?;
                outcome.facts = self
                    .annotate(
                        uid,
                        actor,
                        serde_json::json!({"fiote_configured":true}),
                        now,
                    )
                    .await?;
            }
            Action::CreateAgent { head, operated_by } => {
                let head = head.trim();
                if head.is_empty() {
                    return Err(EngineError::Consequence(
                        "give the Agent a name you will recognise in an assignee list".into(),
                    ));
                }
                let operator = match &operated_by {
                    Some(person) => {
                        let uid = self.resolve(person).await?;
                        let row = store::records::get(&self.store.pool, &uid)
                            .await?
                            .ok_or_else(|| EngineError::UnknownRecord(uid.clone()))?;
                        if row.kind != RecordKind::Person.as_str() {
                            return Err(EngineError::Consequence(
                                "an Agent is operated by a Person".into(),
                            ));
                        }
                        Some(uid)
                    }
                    None => None,
                };
                let actor_concept =
                    store::vocabulary::feature_write(&self.store.pool, "actor").await?;
                for child in ["person", "agent"] {
                    let uid = store::vocabulary::feature_write(&self.store.pool, child).await?;
                    store::concepts::add_parent(&self.store.pool, &uid, &actor_concept).await?;
                }
                let created = self
                    .act(
                        Action::CreateRecord {
                            slug: None,
                            kind: RecordKind::Person,
                            head: head.into(),
                            body: String::new(),
                            quantity: 0.0,
                        },
                        actor.clone(),
                    )
                    .await?;
                let uid = created
                    .created
                    .ok_or_else(|| EngineError::Consequence("Agent was not created".into()))?;
                outcome.facts.extend(created.facts);
                self.act(
                    Action::SetIdentity {
                        subject: uid.clone(),
                        predicate: Some("agent".into()),
                    },
                    actor.clone(),
                )
                .await?;
                if let Some(operator) = operator {
                    store::vocabulary::feature_write(&self.store.pool, "operator").await?;
                    self.act(
                        Action::AssertRecord {
                            subject: uid.clone(),
                            predicate: "operated-by".into(),
                            object: Some(operator),
                            quantity: None,
                            unit: None,
                        },
                        actor.clone(),
                    )
                    .await?;
                }
                outcome.created = Some(uid);
            }
            Action::ActivateFiote {
                target,
                value,
                request_id,
            } => {
                outcome = self
                    .activate_fiote(
                        target,
                        value,
                        request_id,
                        serde_json::json!({"kind":"manual"}),
                        actor.as_deref(),
                    )
                    .await?;
            }
            Action::InspectFioteActivations { target } => {
                let target = self.resolve(&target).await?;
                self.refuse_unreadable(actor.as_deref(), std::slice::from_ref(&target))
                    .await?;
                outcome.data = Some(
                    serde_json::json!({"activations":self.fiote_activations(&target).await?,"limit":256}),
                );
            }
            Action::ReportFioteChild {
                parent,
                thread,
                task,
                state,
            } => {
                let _guard = self.fiote_config_lock.lock().await;
                let parent = self.resolve(&parent).await?;
                let thread = self.resolve(&thread).await?;
                self.refuse_unreadable(actor.as_deref(), &[parent.clone(), thread.clone()])
                    .await?;
                if parent == thread
                    || task.trim().is_empty()
                    || task.len() > 4096
                    || !["working", "waiting", "stopped", "finished", "interrupted"]
                        .contains(&state.as_str())
                {
                    return Err(EngineError::Consequence(
                        "Choose distinct parent/child threads, a task and a valid child state"
                            .into(),
                    ));
                }
                for uid in [&parent, &thread] {
                    if store::records::get(&self.store.pool, uid)
                        .await?
                        .is_none_or(|row| row.kind != "thread")
                    {
                        return Err(EngineError::Consequence(
                            "Child reporting requires conversation threads".into(),
                        ));
                    }
                }
                let mut ancestor = parent.clone();
                let mut depth = 0;
                while let Some(value) =
                    store::records::get_extension(&self.store.pool, &ancestor, "lince.fiote-child")
                        .await?
                {
                    depth += 1;
                    if depth > 32 || value["parent"] == thread {
                        return Err(EngineError::Consequence(
                            "Child sessions cannot form a loop or exceed 32 ancestors".into(),
                        ));
                    }
                    let Some(next) = value["parent"].as_str() else {
                        break;
                    };
                    ancestor = next.into();
                }
                store::records::set_extension(
                    &self.store.pool,
                    &thread,
                    "lince.fiote-child",
                    &serde_json::json!({"parent":parent,"task":task,"state":state}),
                )
                .await?;
                outcome.data = Some(serde_json::json!({"reported":thread,"starts_child":false}));
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
