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

impl Engine {
    pub(crate) async fn set_role_policy(&self, actor: Option<&str>, role: &str, raw: serde_json::Value, expected: i64) -> Result<crate::actions::ActionOutcome, EngineError> {
        self.require_permission(actor, "role:update").await?;
        self.require_permission(actor, "permission:assign").await?;
        let policy: protein::authority::RolePolicy = serde_json::from_value(raw.clone())?;
        if policy.grants.len() > 128 { return Err(EngineError::Consequence("Use at most 128 policy grants".into())); }
        for selector in std::iter::once(&policy.read).chain(policy.grants.iter().map(|grant| &grant.selector)) {
            let query = protein::Protein { source: protein::Source::Record, filter: vec![selector.clone()], fields: None, include: Default::default(), aggregate: None, order: vec![], limit: None };
            protein::validate(&query)?;
        }
        let id = store::auth::role_by_name(&self.store.pool, role).await?.ok_or_else(|| EngineError::Consequence("Choose an existing Role".into()))?;
        if let Some(actor) = actor {
            let viewer = store::auth::principal(&self.store.pool, actor).await?.ok_or_else(|| EngineError::Forbidden("Actor unavailable".into()))?;
            if store::auth::role_permission_keys_by_id(&self.store.pool, id).await?.iter().any(|key| !viewer.permits(key)) { return Err(EngineError::Forbidden("You cannot manage authority beyond your access".into())); }
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_login_on(&mut tx).await?;
        if let Some(actor) = actor {
            let viewer = store::auth::principal_on(&mut tx, actor).await?.ok_or_else(|| EngineError::Forbidden("Actor unavailable".into()))?;
            if !viewer.permits("role:update") || !viewer.permits("permission:assign") || store::auth::role_permission_keys_by_id_on(&mut tx, id).await?.iter().any(|key| !viewer.permits(key)) { return Err(EngineError::Forbidden("Policy management authority changed".into())); }
        }
        let graph = crate::access::policy_graph_on(&mut tx, &[]).await.map_err(crate::record_policy::denied)?;
        let ceiling = crate::record_policy::ceiling(&graph, graph.records.iter().map(|record| record.uid.clone()).collect());
        protein::authority::readable_records(Some(&policy), &graph, &ceiling, &Default::default()).map_err(crate::record_policy::denied)?;
        store::role_policies::set_on(&mut tx, id, &raw, expected).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(Default::default())
    }

    pub(crate) async fn assign_roles(&self, actor: Option<&str>, person: &str, roles: &[String], expected: i64) -> Result<crate::actions::ActionOutcome, EngineError> {
        self.require_permission(actor, "user:assign_role").await?;
        let person = self.resolve(person).await?;
        self.require_manageable_person(actor, &person).await?;
        let mut ids = Vec::new();
        for role in roles {
            ids.push(store::auth::role_by_name(&self.store.pool, role).await?.ok_or_else(|| EngineError::Consequence("Choose existing Roles".into()))?);
        }
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_login_on(&mut tx).await?;
        let recoverable = !store::person_roles::recovery_people_on(&mut tx).await?.is_empty();
        if let Some(actor) = actor {
            let principal = store::auth::principal_on(&mut tx, actor).await?.ok_or_else(|| EngineError::Forbidden("Actor unavailable".into()))?;
            if !principal.permits("user:assign_role") { return Err(EngineError::Forbidden("Role assignment is unavailable".into())); }
            if store::person_roles::permissions_on(&mut tx, &person).await?.iter().any(|key| !principal.permits(key)) { return Err(EngineError::Forbidden("You cannot manage this Actor's authority".into())); }
            for role in &ids {
                if store::auth::role_permission_keys_by_id_on(&mut tx, *role).await?.iter().any(|key| !principal.permits(key)) {
                    return Err(EngineError::Forbidden("You cannot grant authority beyond your own access".into()));
                }
            }
        }
        let access = store::person_roles::replace_on(&mut tx, &person, &ids, expected).await?;
        store::person_roles::require_recovery_on(&mut tx, recoverable).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(crate::actions::ActionOutcome { data: Some(serde_json::json!({"person":person,"revision":access.revision,"roles":roles})), ..Default::default() })
    }
}
