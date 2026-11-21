//! Role, user, and permission catalog reads.

use crate::ProteinError;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use store::{
    Store,
    auth::{list_actors, person_access, principal, role_permission_keys_by_id},
    people::standing,
    person_roles, records, role_policies, roles,
    sqlx::query_scalar,
};
use utils::auth::all_permission_keys;

pub(crate) async fn execute_auth(
    store: &Store,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let pool = &store.pool;
    let actor = match subject {
        Some(uid) => principal(pool, uid).await?,
        None => None,
    };
    let can = |key| subject.is_none() || actor.as_ref().is_some_and(|user| user.permits(key));
    let permissions_visible = can("permission:read");
    let mut out = Vec::new();
    if can("role:read") {
        for role in roles::catalog(pool, permissions_visible).await? {
            let policy = role_policies::get(pool, role.id).await?;
            out.push(json!({
                "kind": "role",
                "id": role.id.to_string(),
                "name": role.name,
                "revision": role.revision,
                "permissions": role.permissions,
                "policy": policy.as_ref().filter(|_| permissions_visible).and_then(|row| row.policy.as_ref()),
                "policy_revision": policy.as_ref().map_or(0, |row| row.revision),
            }));
        }
    }
    if can("user:read") {
        for (uid, username, name, role) in list_actors(pool).await? {
            let person = records::get(pool, &uid).await?;
            let standing = standing(pool, &uid).await?;
            let mut supplied = BTreeMap::<_, Vec<_>>::new();
            let roles = if permissions_visible {
                let mut names = Vec::new();
                for role in person_roles::ids(pool, &uid).await? {
                    let name: String = query_scalar("SELECT name FROM role WHERE id = ?")
                        .bind(role)
                        .fetch_one(pool)
                        .await?;
                    for permission in role_permission_keys_by_id(pool, role).await? {
                        supplied.entry(permission).or_default().push(name.clone());
                    }
                    names.push(name);
                }
                names
            } else {
                person_roles::names(pool, &uid).await?
            };
            let effective_permissions: Vec<_> = supplied
                .into_iter()
                .map(|(permission, roles)| json!({ "permission": permission, "roles": roles }))
                .collect();
            out.push(json!({
                "kind": "user",
                "active": standing.as_ref().is_none_or(|standing| standing.active),
                "deactivated_at": standing.as_ref().and_then(|standing| standing.at.as_deref()),
                "standing_note": standing.as_ref().and_then(|standing| standing.note.as_deref()),
                "id": uid,
                "username": username,
                "has_credentials": !username.is_empty(),
                "effective_permissions": effective_permissions,
                "name": name,
                "role": role,
                "roles": roles,
                "access_revision": person_access(pool, &uid).await?.map_or(0, |access| access.revision),
                "person": uid,
                "person_head": person.as_ref().map(|record| record.head.as_str()),
                "person_slug": person.as_ref().and_then(|record| record.slug.as_deref()),
            }));
        }
    }
    if permissions_visible {
        out.push(json!({ "kind": "permission_catalog", "keys": all_permission_keys() }));
    }
    Ok(out)
}
