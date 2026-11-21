// non-lean-yet

use super::super::{
    Action, ActionOutcome, ControlFlow, DispatchOutcome, Engine, EngineError, invalid_dispatch,
};

impl Engine {
    pub(in super::super) async fn dispatch_access_roles(
        &self,
        action: Action,
        actor: Option<String>,
    ) -> DispatchOutcome {
        let mut outcome = ActionOutcome::default();
        match action {
            Action::RenameRole {
                role,
                expected_revision,
                name,
            } => {
                self.change_role(actor.as_deref(), role, expected_revision, Some(&name))
                    .await?;
            }
            Action::DeleteRole {
                role,
                expected_revision,
            } => {
                self.change_role(actor.as_deref(), role, expected_revision, None)
                    .await?;
            }
            Action::CreateRole { name } => {
                self.require_permission(actor.as_deref(), "role:create")
                    .await?;
                let role_id = store::auth::ensure_role(&self.store.pool, &name).await?;
                outcome.created = Some(role_id.to_string());
            }
            Action::AssignRole { user, role } => {
                self.require_permission(actor.as_deref(), "user:assign_role")
                    .await?;
                let person = self.resolve(&user).await?;
                let expected = store::auth::person_access(&self.store.pool, &person)
                    .await?
                    .map_or(0, |access| access.revision);
                self.assign_roles(actor.as_deref(), &person, &[role], expected)
                    .await?;
            }
            Action::GrantPermission { role, permission } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let (subject, action_name) = permission.split_once(':').ok_or_else(|| {
                    EngineError::Consequence(format!("bad permission `{permission}`"))
                })?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let permission_id =
                    store::auth::ensure_permission(&self.store.pool, subject, action_name).await?;
                store::auth::grant(&self.store.pool, role_id, permission_id).await?;
            }
            Action::RevokePermission { role, permission } => {
                self.require_permission(actor.as_deref(), "permission:assign")
                    .await?;
                let (subject, action_name) = permission.split_once(':').ok_or_else(|| {
                    EngineError::Consequence(format!("bad permission `{permission}`"))
                })?;
                let role_id = store::auth::role_by_name(&self.store.pool, &role)
                    .await?
                    .ok_or_else(|| EngineError::Consequence(format!("unknown role `{role}`")))?;
                let permission_id =
                    store::auth::ensure_permission(&self.store.pool, subject, action_name).await?;
                store::auth::revoke(&self.store.pool, role_id, permission_id).await?;
            }
            _ => return Err(invalid_dispatch()),
        }
        Ok(ControlFlow::Continue(outcome))
    }
}
