//! Role policy composition and the Record read scope it grants.

mod policies;

pub use policies::policies_for_on;

use crate::{
    Protein, ProteinError, Source,
    authority::{Operation, RolePolicy},
    matching_records_for,
    record_schema::predicates_require_body,
};
use serde_json::from_value;
use std::collections::{BTreeSet, HashSet};
use store::{
    Store,
    auth::{role_permission_keys_by_id, role_permission_keys_by_id_on},
    person_roles::{ids, ids_on},
    role_policies,
    sqlx::{Error::Protocol, SqliteConnection},
    visibility::{role_targets, role_targets_on},
};

const READ_PERMISSION: &str = "record:read";

/// Resolves the Role policy for a Person and an optional mutation.
///
/// # Errors
/// Returns an error if the Roles or their policies cannot be read.
pub async fn policy_for(
    store: &Store,
    person: &str,
    operation: Option<Operation>,
) -> Result<Option<RolePolicy>, ProteinError> {
    policy_for_on(&mut *store.pool.acquire().await?, person, operation).await
}

/// Resolves a Role policy on a caller-owned connection.
///
/// # Errors
/// Returns an error if the Roles or their policies cannot be read.
pub async fn policy_for_on(
    connection: &mut SqliteConnection,
    person: &str,
    operation: Option<Operation>,
) -> Result<Option<RolePolicy>, ProteinError> {
    let [policy] = policies_for_on(connection, person, [operation]).await?;
    Ok(policy)
}

/// Returns Records admitted by any of the Person's Roles with read permission.
///
/// # Errors
/// Returns an error if membership, visibility, or policy evaluation fails.
pub async fn readable(store: &Store, person: &str) -> Result<HashSet<String>, ProteinError> {
    let pool = &store.pool;
    let mut result = HashSet::new();
    for role in ids(pool, person).await? {
        if !role_permission_keys_by_id(pool, role)
            .await?
            .iter()
            .any(|key| key == READ_PERMISSION)
        {
            continue;
        }
        let policy: Option<RolePolicy> = role_policies::get(pool, role)
            .await?
            .and_then(|row| row.policy)
            .map(|raw| from_value(raw).map_err(|error| Protocol(error.to_string())))
            .transpose()?;
        let ceiling = role_targets(pool, person, role).await?;
        if let Some(policy) = policy {
            let query = Protein {
                source: Source::Record,
                filter: vec![policy.read],
                fields: None,
                include: Default::default(),
                aggregate: None,
                order: Vec::new(),
                limit: None,
            };
            result.extend(
                matching_records_for::<Option<String>>(
                    store,
                    &query,
                    Some(&ceiling),
                    Some(person),
                    predicates_require_body(&query.filter),
                )
                .await?
                .into_iter()
                .map(|record| record.uid),
            );
        } else {
            result.extend(ceiling);
        }
    }
    Ok(result)
}

/// Lists the Record visibility targets of the Person's read-capable Roles.
///
/// # Errors
/// Returns an error if membership, permissions, or visibility cannot be read.
pub async fn visibility_ceiling(
    store: &Store,
    person: &str,
) -> Result<BTreeSet<String>, ProteinError> {
    visibility_ceiling_on(&mut *store.pool.acquire().await?, person).await
}

/// Lists Role visibility targets on a caller-owned connection.
///
/// # Errors
/// Returns an error if membership, permissions, or visibility cannot be read.
pub async fn visibility_ceiling_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<BTreeSet<String>, ProteinError> {
    let mut result = BTreeSet::new();
    for role in ids_on(connection, person).await? {
        if role_permission_keys_by_id_on(connection, role)
            .await?
            .iter()
            .any(|key| key == READ_PERMISSION)
        {
            result.extend(role_targets_on(connection, person, role).await?);
        }
    }
    Ok(result)
}
