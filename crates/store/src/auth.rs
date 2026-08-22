//! The permission/role workflow tables, and the local credential that lets a
//! Person log in here.
//!
//! There is ONE human reference in Lince: the Person record. A
//! `person_credential` row is not a second identity — it is a way to prove you
//! are one of them over HTTP, holding a username, a password hash and a role.
//! Persons without one are perfectly ordinary: your contacts, and the Person a
//! `GrantOrganLogin` names for a remote Organ, all exist with no credential and
//! act through the iroh handshake instead (see `store::logins`).
//!
//! Credentials are LOCAL AND NEVER SYNCED. Person records travel to contacts;
//! password hashes must not, which is the whole reason this is a side table
//! rather than columns on the record.
//!
//! Roles are native structured state, not Ledger records: a `role` groups
//! `permission`s via `role_permission`. The `admin` role is the
//! all-permissions role; the first-run bootstrap creates the initial admin
//! (see `web` cell bootstrap).

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

/// Move a Person to a different role (idempotent).
pub async fn set_user_role(
    pool: &SqlitePool,
    person_uid: &str,
    role_id: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE person_credential SET role_id = ?, updated_at = CURRENT_TIMESTAMP WHERE person_uid = ?")
        .bind(role_id)
        .bind(person_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Does this Person have a way to log in here?
///
/// Replaces the old `user_for_person`: there is no separate user to find, only
/// the question of whether a credential exists — which is what every caller
/// actually wanted to know.
pub async fn has_credential(pool: &SqlitePool, person_uid: &str) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM person_credential WHERE person_uid = ?")
            .bind(person_uid)
            .fetch_optional(pool)
            .await?
            .is_some(),
    )
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

/// Every Person who can log in here, with their role name (no password hash —
/// this is a read surface for the role-management sand, never an auth check).
/// The display name is the Person's own `head`, because there is no second
/// place for a human's name to live.
pub async fn list_users(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String, String)>, StoreError> {
    sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT c.person_uid, c.username, p.head, r.name
         FROM person_credential c
         JOIN record p ON p.uid = c.person_uid
         LEFT JOIN role r ON r.id = c.role_id
         ORDER BY c.username",
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|(uid, username, name, role)| (uid, username, name, role.unwrap_or_default()))
            .collect()
    })
}

/// Does any Person currently hold the admin role?
pub async fn admin_exists(pool: &SqlitePool) -> Result<bool, StoreError> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM person_credential
         WHERE role_id = (SELECT id FROM role WHERE name = ?)",
    )
    .bind(ADMIN_ROLE)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Every Person holding the admin role, active or not.
///
/// The caller filters by standing — this stays a plain membership query so it
/// cannot silently disagree with `admin_exists` about who an admin is.
pub async fn admins(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar::<_, String>(
        "SELECT person_uid FROM person_credential
         WHERE role_id = (SELECT id FROM role WHERE name = ?)",
    )
    .bind(ADMIN_ROLE)
    .fetch_all(pool)
    .await
}

/// Create a Person AND their way to log in, returning the Person's uid.
///
/// The common case, and the only one that used to be expressible as
/// "create a user": a human who exists here and can sign in. Kept in one place
/// because the two halves must not drift apart — a credential whose
/// `person_uid` names no record is unrepresentable, and this is what makes
/// that true at every call site.
pub async fn create_person_login(
    pool: &SqlitePool,
    name: &str,
    username: &str,
    password_hash: &str,
    role_id: i64,
) -> Result<String, StoreError> {
    let person = crate::records::create(
        pool,
        crate::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: name,
            body: "",
            quantity: crate::exact::zero(),
        },
    )
    .await?;
    create_credential(pool, &person.uid, username, password_hash, role_id).await
}

/// Give an existing Person a way to log in, returning their uid.
///
/// `password_hash` must be a hash from `utils::auth::hash_password` — this
/// layer never sees plaintext. The Person must already exist: creating one is
/// the engine's job (records carry op-log history and sync), which is exactly
/// why this function takes a uid rather than a name.
pub async fn create_credential(
    pool: &SqlitePool,
    person_uid: &str,
    username: &str,
    password_hash: &str,
    role_id: i64,
) -> Result<String, StoreError> {
    sqlx::query(
        "INSERT INTO person_credential (person_uid, username, password_hash, role_id)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(person_uid) DO UPDATE SET
             username = excluded.username,
             password_hash = excluded.password_hash,
             role_id = excluded.role_id,
             updated_at = CURRENT_TIMESTAMP",
    )
    .bind(person_uid)
    .bind(username)
    .bind(password_hash)
    .bind(role_id)
    .execute(pool)
    .await?;
    Ok(person_uid.to_string())
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

/// A Person who can log in here. `uid` IS the Person's record uid — the same
/// value that lands on facts as the actor, gates reads in `visible_targets`,
/// and travels as the transport session's subject. One human, one id.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthUser {
    pub uid: String,
    pub username: String,
    pub name: String,
    pub password_hash: String,
    pub role_id: i64,
    pub role: String,
    pub permissions: Vec<String>,
}

const CREDENTIAL_SELECT: &str = "
    SELECT c.person_uid, c.username, p.head, c.password_hash, c.role_id, r.name
    FROM person_credential c
    JOIN record p ON p.uid = c.person_uid
    LEFT JOIN role r ON r.id = c.role_id
";

async fn credential_row(
    pool: &SqlitePool,
    column: &str,
    value: &str,
) -> Result<Option<AuthUser>, StoreError> {
    let sql = format!("{CREDENTIAL_SELECT} WHERE {column} = ?");
    let Some((uid, username, name, password_hash, role_id, role)) =
        sqlx::query_as::<_, (String, String, String, String, Option<i64>, Option<String>)>(&sql)
            .bind(value)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };

    let role = role.unwrap_or_default();
    let permissions = if role.is_empty() {
        Vec::new()
    } else {
        role_permission_keys(pool, &role).await?
    };

    Ok(Some(AuthUser {
        uid,
        username,
        name,
        password_hash,
        role_id: role_id.unwrap_or_default(),
        role,
        permissions,
    }))
}

pub async fn user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<AuthUser>, StoreError> {
    // `column` is a fixed literal at both call sites, never caller input.
    credential_row(pool, "c.username", username).await
}

/// Look a Person up by uid. Returns `None` for a Person with no credential —
/// a contact, or the Person a `GrantOrganLogin` named — which is a normal
/// state, not an error: they simply cannot log in with a password here.
pub async fn user_by_uid(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Option<AuthUser>, StoreError> {
    credential_row(pool, "c.person_uid", person_uid).await
}
