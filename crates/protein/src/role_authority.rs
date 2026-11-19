use crate::{
    Protein, ProteinError, Source,
    authority::{Operation, RolePolicy},
};
use std::collections::HashSet;

mod policies;
pub use policies::policies_for_on;

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
    let [policy] = policies_for_on(connection, person, [operation]).await?;
    Ok(policy)
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
            result.extend(
                crate::matching_records_for::<Option<String>>(
                    store,
                    &query,
                    Some(&ceiling),
                    Some(person),
                    crate::record_schema::predicates_require_body(&query.filter),
                )
                .await?
                .into_iter()
                .map(|record| record.uid),
            );
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
