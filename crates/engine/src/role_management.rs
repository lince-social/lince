use crate::{Engine, EngineError};

impl Engine {
    pub(crate) async fn change_role(
        &self,
        actor: Option<&str>,
        role: i64,
        expected_revision: i64,
        name: Option<&str>,
    ) -> Result<(), EngineError> {
        let permission = if name.is_some() {
            "role:update"
        } else {
            "role:delete"
        };
        self.require_permission(actor, permission).await?;
        if role <= 0
            || expected_revision <= 0
            || name.is_some_and(|name| !store::roles::valid_name(name))
        {
            return Err(EngineError::Consequence(
                "Invalid Role identity, revision or name".into(),
            ));
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_login_on(&mut tx).await?;
        let target =
            store::roles::get_on(&mut tx, role)
                .await?
                .ok_or_else(|| EngineError::Conflict {
                    code: "role_changed",
                    message: "This Role is no longer available. Refresh the list.".into(),
                })?;
        if let Some(actor) = actor {
            let viewer = store::auth::principal_on(&mut tx, actor)
                .await?
                .ok_or_else(|| EngineError::Forbidden("Unrecognized Actor".into()))?;
            if !viewer.permits(permission)
                || target.permissions.iter().any(|key| !viewer.permits(key))
            {
                return Err(EngineError::Forbidden(
                    "You cannot manage a Role with greater access".into(),
                ));
            }
        }
        if target.name == store::auth::ADMIN_ROLE || name == Some(store::auth::ADMIN_ROLE) {
            return Err(EngineError::Forbidden(
                "The admin Role name is protected to preserve recovery access".into(),
            ));
        }
        if target.revision != expected_revision {
            return Err(EngineError::Conflict {
                code: "role_changed",
                message: "This Role changed. Refresh and review it before trying again.".into(),
            });
        }
        if let Some(name) = name {
            if store::roles::name_exists_on(&mut tx, name, role).await? {
                return Err(EngineError::Conflict {
                    code: "role_name_taken",
                    message: "A Role with that name already exists.".into(),
                });
            }
            store::roles::rename_on(&mut tx, role, name).await?;
        } else {
            if store::roles::assigned_on(&mut tx, role).await? {
                return Err(EngineError::Conflict { code: "role_assigned", message: "This Role is still assigned. Reassign every Person first, including inactive people and people without logins.".into() });
            }
            store::roles::delete_on(&mut tx, role).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
