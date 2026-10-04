use crate::{
    Predicate, Protein, ProteinError, Source,
    authority::{Operation, RolePolicy},
};
use std::collections::HashSet;

fn permission(operation: Option<Operation>) -> &'static str {
    match operation {
        None => "record:read",
        Some(Operation::Create) => "record:create",
        Some(Operation::Update | Operation::Restore) => "record:update",
        Some(Operation::Delete) => "record:delete",
    }
}

pub async fn policy_for(
    store: &store::Store,
    person: &str,
    operation: Option<Operation>,
) -> Result<Option<RolePolicy>, ProteinError> {
    policy_for_on(&mut *store.pool.acquire().await?, person, operation).await
}

pub async fn policy_for_on(
    connection: &mut store::sqlx::SqliteConnection,
    person: &str,
    operation: Option<Operation>,
) -> Result<Option<RolePolicy>, ProteinError> {
    let roles = store::person_roles::ids_on(connection, person).await?;
    let mut read = Vec::new();
    let mut grants = Vec::new();
    let mut unrestricted_read = false;
    let mut unrestricted_write = false;
    for role in roles {
        let permissions = store::auth::role_permission_keys_by_id_on(connection, role).await?;
        let allows = |key: &str| permissions.iter().any(|permission| permission == key);
        let write_allowed = allows(permission(operation))
            || operation == Some(Operation::Delete) && allows("record:delete_own");
        let raw = store::role_policies::get_on(connection, role)
            .await?
            .and_then(|row| row.policy);
        if let Some(raw) = raw {
            let policy: RolePolicy = serde_json::from_value(raw)
                .map_err(|error| store::StoreError::Protocol(error.to_string()))?;
            if allows("record:read") {
                read.push(policy.read);
            }
            if write_allowed {
                grants.extend(policy.grants.into_iter().filter(|grant| {
                    operation.is_none_or(|operation| grant.operation == operation)
                }));
            }
        } else {
            unrestricted_read |= allows("record:read");
            unrestricted_write |= operation.is_some() && write_allowed;
        }
    }
    if unrestricted_write || operation.is_none() && unrestricted_read {
        return Ok(None);
    }
    Ok(Some(RolePolicy {
        read: if unrestricted_read {
            Predicate::All(vec![])
        } else {
            Predicate::Any(read)
        },
        grants,
    }))
}

pub async fn readable(store: &store::Store, person: &str) -> Result<HashSet<String>, ProteinError> {
    let mut result = HashSet::new();
    for role in store::person_roles::ids(&store.pool, person).await? {
        if !store::auth::role_permission_keys_by_id(&store.pool, role)
            .await?
            .iter()
            .any(|key| key == "record:read")
        {
            continue;
        }
        let raw = store::role_policies::get(&store.pool, role)
            .await?
            .and_then(|row| row.policy);
        if let Some(raw) = raw {
            let policy: RolePolicy = serde_json::from_value(raw)
                .map_err(|error| store::StoreError::Protocol(error.to_string()))?;
            let ceiling = store::visibility::role_targets(&store.pool, person, role).await?;
            let query = Protein {
                source: Source::Record,
                filter: vec![policy.read],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: vec![],
                limit: None,
            };
            result.extend(crate::matching_among(store, &query, &ceiling).await?);
        } else {
            result.extend(store::visibility::role_targets(&store.pool, person, role).await?);
        }
    }
    Ok(result)
}

pub async fn visibility_ceiling(
    store: &store::Store,
    person: &str,
) -> Result<std::collections::BTreeSet<String>, ProteinError> {
    visibility_ceiling_on(&mut *store.pool.acquire().await?, person).await
}

pub async fn visibility_ceiling_on(
    connection: &mut store::sqlx::SqliteConnection,
    person: &str,
) -> Result<std::collections::BTreeSet<String>, ProteinError> {
    let mut result = std::collections::BTreeSet::new();
    for role in store::person_roles::ids_on(connection, person).await? {
        if store::auth::role_permission_keys_by_id_on(connection, role)
            .await?
            .iter()
            .any(|key| key == "record:read")
        {
            result.extend(store::visibility::role_targets_on(connection, person, role).await?);
        }
    }
    Ok(result)
}
