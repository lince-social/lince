// non-lean-yet

use super::super::{
    Action, ActionOutcome, Cause, ControlFlow, DateTime, DispatchOutcome, Engine, EngineError,
    NewFact, Utc, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_access_accounts(
        &self,
        action: Action,
        actor: Option<String>,
        now: DateTime<Utc>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::CreateUser {
                username,
                name,
                password,
                role,
            } => {
                self.require_permission(actor.as_deref(), "user:create")
                    .await?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let password_hash = self
                    .passwords
                    .hash(
                        crate::private_password::PasswordInput::new(password.into_bytes())
                            .map_err(|error| EngineError::Consequence(error.to_string()))?,
                    )
                    .await
                    .map_err(|error| EngineError::Consequence(error.to_string()))?;
                let person_uid = store::auth::create_person_login(
                    &self.store.pool,
                    &name,
                    &username,
                    password_hash.as_phc(),
                    role_id,
                )
                .await?;
                outcome.facts = self
                    .append(
                        NewFact {
                            actor_uid: actor,
                            ..NewFact::quantity_f64(person_uid.clone(), 0.0, Cause::user_edit())
                        },
                        now,
                    )
                    .await?;
                outcome.created = Some(person_uid);
            }
            Action::UpdateUser {
                user,
                username,
                name,
                password,
            } => {
                self.change_login(actor.as_deref(), &user, Some((username, name, password)))
                    .await?;
            }
            Action::DeleteUser { user } => {
                self.change_login(actor.as_deref(), &user, None).await?;
            }
            Action::SetPersonStanding {
                person,
                active,
                note,
            } => {
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
                if actor.as_deref() == Some(person_uid.as_str()) && !active {
                    return Err(EngineError::Consequence(
                        "you cannot deactivate yourself — ask another admin".into(),
                    ));
                }
                if !active {
                    let admins = store::auth::admins(&self.store.pool).await?;
                    if admins.iter().any(|admin| admin == &person_uid) {
                        let mut others = 0;
                        for admin in admins.iter().filter(|admin| *admin != &person_uid) {
                            if store::people::is_active(&self.store.pool, admin).await? {
                                others += 1;
                            }
                        }
                        if others == 0 {
                            return Err(EngineError::Consequence(
                                "This is the last active Actor with recovery capabilities. Assign those capabilities to another active Actor first."
                                    .into(),
                            ));
                        }
                    }
                }
                if active {
                    store::people::reactivate(&self.store.pool, &person_uid).await?;
                } else {
                    store::people::deactivate(
                        &self.store.pool,
                        &person_uid,
                        &now.to_rfc3339(),
                        note.as_deref(),
                    )
                    .await?;
                }
                outcome.created = Some(person_uid);
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
