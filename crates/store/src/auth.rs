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
    pub password_hash: String,
    pub role_id: i64,
    pub role: String,
    pub permissions: Vec<String>,
}

pub async fn user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<AuthUser>, StoreError> {
    let Some(row) = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>)>(
        "
        SELECT u.id, u.username, u.password_hash, u.role_id, r.name
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

    let (id, username, password_hash, role_id, role) = row;
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
        password_hash,
        role_id,
        role,
        permissions,
    }))
}

pub async fn user_by_id(pool: &SqlitePool, user_id: i64) -> Result<Option<AuthUser>, StoreError> {
    let Some(row) = sqlx::query_as::<_, (i64, String, String, Option<i64>, Option<String>)>(
        "
        SELECT u.id, u.username, u.password_hash, u.role_id, r.name
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

    let (id, username, password_hash, role_id, role) = row;
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
        password_hash,
        role_id,
        role,
        permissions,
    }))
}
