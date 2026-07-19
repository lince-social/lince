//! The permission/role/user workflow tables. Native structured state (not Ledger
//! records): a `role` groups `permission`s via `role_permission`, and an
//! `app_user` logs in with a hashed password and holds one role. The `admin`
//! role is the all-permissions role; the first-run bootstrap creates the initial
//! admin (see `web` cell bootstrap).

use sqlx::SqlitePool;

use crate::StoreError;

pub const ADMIN_ROLE: &str = "admin";
pub const LINCE_ROLE: &str = "lince";

/// Ensure a role exists; return its id.
pub async fn ensure_role(pool: &SqlitePool, name: &str) -> Result<i64, StoreError> {
    sqlx::query("INSERT OR IGNORE INTO role (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;
    sqlx::query_scalar::<_, i64>("SELECT id FROM role WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await
}

/// Ensure a permission exists; return its id.
pub async fn ensure_permission(
    pool: &SqlitePool,
    subject: &str,
    action: &str,
) -> Result<i64, StoreError> {
    sqlx::query("INSERT OR IGNORE INTO permission (subject, action) VALUES (?, ?)")
        .bind(subject)
        .bind(action)
        .execute(pool)
        .await?;
    sqlx::query_scalar::<_, i64>("SELECT id FROM permission WHERE subject = ? AND action = ?")
        .bind(subject)
        .bind(action)
        .fetch_one(pool)
        .await
}

/// Grant a permission to a role (idempotent).
pub async fn grant(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    sqlx::query("INSERT OR IGNORE INTO role_permission (role_id, permission_id) VALUES (?, ?)")
        .bind(role_id)
        .bind(permission_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Revoke a permission from a role (idempotent — a no-op if it wasn't granted).
pub async fn revoke(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM role_permission WHERE role_id = ? AND permission_id = ?")
        .bind(role_id)
        .bind(permission_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// A role's id by name — read-only (unlike `ensure_role`, never creates one).
pub async fn role_by_name(pool: &SqlitePool, name: &str) -> Result<Option<i64>, StoreError> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM role WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
}

/// Move a user to a different role (idempotent).
pub async fn set_user_role(
    pool: &SqlitePool,
    user_id: i64,
    role_id: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE app_user SET role_id = ? WHERE id = ?")
        .bind(role_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bind an authenticated app user to the Person that represents them in the
/// Ledger. The schema keeps both sides one-to-one; reassignment is explicit,
/// while attempting to claim another user's Person remains a constraint error.
pub async fn set_user_person(
    pool: &SqlitePool,
    user_id: i64,
    person_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO app_user_person (user_id, person_uid) VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
             person_uid = excluded.person_uid,
             assigned_at = CURRENT_TIMESTAMP",
    )
    .bind(user_id)
    .bind(person_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// The Ledger Person controlled by an app user, when an administrator has
/// established that identity binding.
pub async fn person_for_user(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT person_uid FROM app_user_person WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
}

/// The app user controlling a Person. Useful for capability explanations and
/// for rejecting attempts to assign a Person that is already represented.
pub async fn user_for_person(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Option<i64>, StoreError> {
    sqlx::query_scalar("SELECT user_id FROM app_user_person WHERE person_uid = ?")
        .bind(person_uid)
        .fetch_optional(pool)
        .await
}

/// Every role with its granted permission keys — the role-management sand's
/// full listing (there's no Ledger record for a role, so this is the only way
/// to read them; backs the new `protein::Source::Auth`).
pub async fn list_roles(pool: &SqlitePool) -> Result<Vec<(i64, String, Vec<String>)>, StoreError> {
    let roles = sqlx::query_as::<_, (i64, String)>("SELECT id, name FROM role ORDER BY name")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(roles.len());
    for (id, name) in roles {
        let permissions = role_permission_keys(pool, &name).await?;
        out.push((id, name, permissions));
    }
    Ok(out)
}

/// Every user with their role name (no password hash — this is a read
/// surface for the role-management sand, never an auth check).
pub async fn list_users(
    pool: &SqlitePool,
) -> Result<Vec<(i64, String, String, String)>, StoreError> {
    sqlx::query_as::<_, (i64, String, String, Option<String>)>(
        "SELECT u.id, u.username, u.name, r.name
         FROM app_user u
         LEFT JOIN role r ON r.id = u.role_id
         ORDER BY u.username",
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|(id, username, name, role)| (id, username, name, role.unwrap_or_default()))
            .collect()
    })
}

/// Does any user currently hold the admin role?
pub async fn admin_exists(pool: &SqlitePool) -> Result<bool, StoreError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM app_user
         WHERE role_id = (SELECT id FROM role WHERE name = ?)",
    )
    .bind(ADMIN_ROLE)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Create a user holding `role_id`, returning its id. `password_hash` must be a
/// hash from `utils::auth::hash_password` — this layer never sees plaintext.
pub async fn create_user(
    pool: &SqlitePool,
    name: &str,
    username: &str,
    password_hash: &str,
    role_id: i64,
) -> Result<i64, StoreError> {
    Ok(sqlx::query(
        "INSERT INTO app_user (name, username, password_hash, role_id) VALUES (?, ?, ?, ?)",
    )
    .bind(name)
    .bind(username)
    .bind(password_hash)
    .bind(role_id)
    .execute(pool)
    .await?
    .last_insert_rowid())
}

/// The permission keys granted to a role, as `"subject:action"` strings.
pub async fn role_permission_keys(
    pool: &SqlitePool,
    role: &str,
) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar::<_, String>(
        "SELECT p.subject || ':' || p.action
         FROM permission p
         JOIN role_permission rp ON rp.permission_id = p.id
         JOIN role r ON r.id = rp.role_id
         WHERE r.name = ?
         ORDER BY p.subject, p.action",
    )
    .bind(role)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthUser {
    pub id: i64,
    pub username: String,
    pub name: String,
    pub password_hash: String,
    pub role_id: i64,
    pub role: String,
    pub permissions: Vec<String>,
}

pub async fn user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<AuthUser>, StoreError> {
    let Some(row) =
        sqlx::query_as::<_, (i64, String, String, String, Option<i64>, Option<String>)>(
            "
        SELECT u.id, u.username, u.name, u.password_hash, u.role_id, r.name
        FROM app_user u
        LEFT JOIN role r ON r.id = u.role_id
        WHERE u.username = ?
        ",
        )
        .bind(username)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let (id, username, name, password_hash, role_id, role) = row;
    let role_id = role_id.unwrap_or_default();
    let role = role.unwrap_or_default();
    let permissions = if role.is_empty() {
        Vec::new()
    } else {
        role_permission_keys(pool, &role).await?
    };

    Ok(Some(AuthUser {
        id,
        username,
        name,
        password_hash,
        role_id,
        role,
        permissions,
    }))
}

pub async fn user_by_id(pool: &SqlitePool, user_id: i64) -> Result<Option<AuthUser>, StoreError> {
    let Some(row) =
        sqlx::query_as::<_, (i64, String, String, String, Option<i64>, Option<String>)>(
            "
        SELECT u.id, u.username, u.name, u.password_hash, u.role_id, r.name
        FROM app_user u
        LEFT JOIN role r ON r.id = u.role_id
        WHERE u.id = ?
        ",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    let (id, username, name, password_hash, role_id, role) = row;
    let role_id = role_id.unwrap_or_default();
    let role = role.unwrap_or_default();
    let permissions = if role.is_empty() {
        Vec::new()
    } else {
        role_permission_keys(pool, &role).await?
    };

    Ok(Some(AuthUser {
        id,
        username,
        name,
        password_hash,
        role_id,
        role,
        permissions,
    }))
}
