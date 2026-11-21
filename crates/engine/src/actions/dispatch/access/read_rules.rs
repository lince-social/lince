// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DispatchOutcome, Engine, EngineError, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_access_read_rules(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> DispatchOutcome {
        let outcome = ActionOutcome::default();
        match action {
            Action::SetPersonReadFilter { person, filter } => {
                self.require_permission(actor.as_deref(), "user:update")
                    .await?;
                let person_uid = self.resolve(&person).await?;
                let record = store::records::get(&self.store.pool, &person_uid)
                    .await?
                    .ok_or_else(|| {
                        EngineError::Consequence(format!("no such Person `{person}`"))
                    })?;
                if record.kind != "person" {
                    return Err(EngineError::Consequence(format!(
                        "`{person}` is a {} record, not a Person",
                        record.kind
                    )));
                }
                self.set_read_filter(&person_uid, filter.as_ref()).await?;
            }
            Action::SetRoleReadRules {
                role,
                rules,
                expected_revision,
            } => {
                self.require_permission(actor.as_deref(), "role:update")
                    .await?;
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                rules.validate()?;
                let mut pending = vec![&rules.allow, &rules.block];
                while let Some(predicate) = pending.pop() {
                    match predicate {
                        protein::Predicate::All(children) | protein::Predicate::Any(children) => {
                            pending.extend(children)
                        }
                        protein::Predicate::Not(child) => pending.push(child),
                        protein::Predicate::ConceptIn(uid) => {
                            if store::concepts::resolve(&self.store.pool, uid)
                                .await?
                                .is_none()
                            {
                                return Err(EngineError::Consequence(
                                    "Choose an existing tag".into(),
                                ));
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                let id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence("Unknown role".into()))?;
                let previous = store::role_policies::get(&self.store.pool, id).await?;
                if previous.as_ref().map_or(0, |row| row.revision) != expected_revision {
                    return Err(EngineError::Conflict {
                        code: "role_policy_changed",
                        message: "The role rules changed. Reload them before saving.".into(),
                    });
                }
                let mut policy = if let Some(value) = previous.and_then(|row| row.policy) {
                    serde_json::from_value::<protein::authority::RolePolicy>(value)
                        .map_err(|error| EngineError::Consequence(error.to_string()))?
                } else {
                    protein::authority::RolePolicy {
                        read: protein::Predicate::All(vec![]),
                        grants: vec![],
                    }
                };
                policy.read = rules.predicate();
                let value = serde_json::to_value(policy)
                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                store::role_policies::set(&self.store.pool, id, &value, expected_revision).await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
