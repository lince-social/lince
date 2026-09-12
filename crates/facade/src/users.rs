use crate::{
    State,
    auth::{self, Failure, internal},
    records::permits,
};
use axum::{
    Json,
    extract::State as Extract,
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
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

#[derive(Deserialize)]
pub(crate) struct Change {
    uid: String,
    #[serde(default)]
    delete: bool,
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    password: String,
}

pub(crate) async fn change(
    Extract(state): Extract<State>,
    headers: HeaderMap,
    Json(input): Json<Change>,
) -> Result<Json<Value>, Failure> {
    auth::same_origin(&headers)?;
    let _management = state.auth.management.lock().await;
    let viewer = auth::viewer(&state, &headers)
        .await?
        .ok_or_else(auth::refused)?;
    let denied = || (StatusCode::FORBIDDEN, "You cannot make this change.".into());
    if !permits(
        Some(&viewer),
        if input.delete {
            "user:delete"
        } else {
            "user:update"
        },
    ) {
        return Err(denied());
    }
    let generation = {
        let mut connection = state.cell.store.pool.acquire().await.map_err(internal)?;
        store::auth::credential_generation_on(&mut connection, &input.uid)
            .await
            .map_err(internal)?
    };
    let target = store::auth::user_by_uid(&state.cell.store.pool, &input.uid)
        .await
        .map_err(internal)?
        .ok_or_else(denied)?;
    if target.role == "admin" && viewer.role != "admin" {
        return Err(denied());
    }
    if target
        .permissions
        .iter()
        .any(|key| !permits(Some(&viewer), key))
    {
        return Err(denied());
    }
    if input.delete && (target.uid == viewer.uid || target.role == "admin") {
        return Err((
            StatusCode::BAD_REQUEST,
            "Assign another role before deleting an admin. You cannot delete your own login."
                .into(),
        ));
    }
    if !input.delete
        && (input.username.trim().is_empty()
            || input.username.len() > 256
            || input.name.len() > 500
            || input.password.len() > 1024)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Enter a username and valid account details.".into(),
        ));
    }
    let hash = if input.password.is_empty() {
        target.password_hash
    } else {
        let _permit = state
            .auth
            .passwords
            .clone()
            .acquire_owned()
            .await
            .map_err(internal)?;
        tokio::task::spawn_blocking(move || utils::auth::hash_password(&input.password))
            .await
            .map_err(internal)?
            .map_err(internal)?
    };
    let mut tx = store::write_tx(&state.cell.store.pool)
        .await
        .map_err(internal)?;
    let target_access = store::auth::person_access_on(&mut tx, &input.uid)
        .await
        .map_err(internal)?;
    let viewer_access = store::auth::person_access_on(&mut tx, &viewer.uid)
        .await
        .map_err(internal)?;
    if target_access.and_then(|a| a.role_id) != Some(target.role_id)
        || viewer_access.and_then(|a| a.role_id) != Some(viewer.role_id)
    {
        return Err(denied());
    }
    let current_permissions = store::auth::role_permission_keys_by_id_on(&mut tx, viewer.role_id)
        .await
        .map_err(internal)?;
    if viewer.role != "admin"
        && !current_permissions.iter().any(|key| {
            key == if input.delete {
                "user:delete"
            } else {
                "user:update"
            }
        })
    {
        return Err(denied());
    }
    if input.delete {
        store::auth::remove_credential_on(&mut tx, &input.uid, generation)
            .await
            .map_err(internal)?;
    } else {
        store::auth::replace_credential_on(
            &mut tx,
            &input.uid,
            input.username.trim(),
            &hash,
            generation,
        )
        .await
        .map_err(internal)?;
        store::records::set_authoring_text_on(&mut tx, &input.uid, Some(input.name.trim()), None)
            .await
            .map_err(internal)?;
    }
    tx.commit().await.map_err(internal)?;
    Ok(Json(json!({"ok":true})))
}
