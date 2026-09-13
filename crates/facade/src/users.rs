use crate::{
    State,
    auth::{Failure, internal},
    records::permits,
};
use serde_json::{Value, json};

pub(crate) async fn snapshot(
    state: &State,
    user: Option<&store::auth::AuthUser>,
) -> Result<Value, Failure> {
    let can = |key| user.is_some() && permits(user, key);
    let mut users = Vec::new();
    if can("user:read") {
        for (uid, username, name, role) in store::auth::list_users(&state.cell.store.pool)
            .await
            .map_err(internal)?
        {
            let access = store::auth::person_access(&state.cell.store.pool, &uid)
                .await
                .map_err(internal)?;
            users.push(json!({"uid":uid,"username":username,"name":name,"role":role,"filter":access.and_then(|a| a.read_filter)}));
        }
    }
    let mut roles = Vec::new();
    if can("role:read") {
        for (id, name, permissions) in store::auth::list_roles(&state.cell.store.pool)
            .await
            .map_err(internal)?
        {
            let saved = store::role_policies::get(&state.cell.store.pool, id)
                .await
                .map_err(internal)?;
            let revision = saved.as_ref().map_or(0, |row| row.revision);
            let rules = if let Some(value) = saved.and_then(|row| row.policy) {
                let policy: protein::authority::RolePolicy =
                    serde_json::from_value(value).map_err(internal)?;
                protein::read_rules::ReadRules::from_predicate(policy.read)
            } else {
                protein::read_rules::ReadRules::default()
            };
            roles.push(json!({"name":name,"permissions":if can("permission:read") {permissions} else {vec![]},"rules":rules,"revision":revision}));
        }
    }
    let concepts = if can("role:read") || permits(user, "record:create") {
        store::concepts::list_all(&state.cell.store.pool)
            .await
            .map_err(internal)?
            .into_iter()
            .map(|concept| json!({"uid":concept.uid,"name":concept.canonical_name}))
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    Ok(
        json!({"users":users,"roles":roles,"filterconcepts":concepts,"canrules":crate::settings::administrator(user),"permissions":if can("permission:read") { utils::auth::all_permission_keys() } else { Vec::new() },
        "canusers":can("user:read"),"canusercreate":can("user:create"),"canuserupdate":can("user:update"),"canuserdelete":can("user:delete"),
        "canassign":can("user:assign_role"),"canrolecreate":can("role:create"),"canpermissions":can("permission:assign")}),
    )
}
