use sqlx::{SqliteConnection, SqlitePool, query, query_as, query_scalar};

use super::protocol;
use crate::{
    StoreError,
    person_roles::{recovery_people_on, require_recovery_on},
    write_tx,
};

/// Name of the built-in administrator role.
pub const ADMIN_ROLE: &str = "admin";
/// Name of the built-in Lince role.
pub const LINCE_ROLE: &str = "lince";

/// Maximum retained permission count per Role.
pub const MAX_ROLE_PERMISSIONS: usize = 1024;
/// Maximum combined byte length of a stored permission key.
pub const MAX_PERMISSION_KEY_BYTES: usize = 256;
/// Maximum total permission key bytes per Role.
pub const MAX_ROLE_PERMISSION_BYTES: usize = 64 * 1024;

/// Returns the ID of the named Role, creating it if needed.
pub async fn ensure_role(pool: &SqlitePool, name: &str) -> Result<i64, StoreError> {
    let mut connection = pool.acquire().await?;
    ensure_role_on(&mut connection, name)
        .await
        .map(|(role_id, _)| role_id)
}

/// Ensures the named Role exists and reports whether it was inserted.
pub async fn ensure_role_on(
    connection: &mut SqliteConnection,
    name: &str,
) -> Result<(i64, bool), StoreError> {
    let inserted = query("INSERT OR IGNORE INTO role (name) VALUES (?)")
        .bind(name)
        .execute(&mut *connection)
        .await?
        .rows_affected()
        == 1;
    let role_id = query_scalar::<_, i64>("SELECT id FROM role WHERE name = ?")
        .bind(name)
        .fetch_one(&mut *connection)
        .await?;
    Ok((role_id, inserted))
}

/// Ensures a permission exists for the subject and action.
pub async fn ensure_permission(
    pool: &SqlitePool,
    subject: &str,
    action: &str,
) -> Result<i64, StoreError> {
    let mut connection = pool.acquire().await?;
    ensure_permission_on(&mut connection, subject, action).await
}

/// Ensures a permission on the supplied connection.
pub async fn ensure_permission_on(
    connection: &mut SqliteConnection,
    subject: &str,
    action: &str,
) -> Result<i64, StoreError> {
    query("INSERT OR IGNORE INTO permission (subject, action) VALUES (?, ?)")
        .bind(subject)
        .bind(action)
        .execute(&mut *connection)
        .await?;
    query_scalar::<_, i64>("SELECT id FROM permission WHERE subject = ? AND action = ?")
        .bind(subject)
        .bind(action)
        .fetch_one(&mut *connection)
        .await
}

/// Adds the permission to the Role if absent.
pub async fn grant(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    let mut connection = pool.acquire().await?;
    grant_on(&mut connection, role_id, permission_id).await
}

/// Adds the permission to the Role on the supplied connection.
pub async fn grant_on(
    connection: &mut SqliteConnection,
    role_id: i64,
    permission_id: i64,
) -> Result<(), StoreError> {
    query("INSERT OR IGNORE INTO role_permission (role_id, permission_id) VALUES (?, ?)")
        .bind(role_id)
        .bind(permission_id)
        .execute(&mut *connection)
        .await?;
    Ok(())
}

/// Removes a permission while preserving an existing recovery path.
///
/// # Errors
/// Refuses removing the last recovery path and propagates database failures.
pub async fn revoke(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    let recoverable = !recovery_people_on(&mut tx).await?.is_empty();
    query("DELETE FROM role_permission WHERE role_id = ? AND permission_id = ?")
        .bind(role_id)
        .bind(permission_id)
        .execute(&mut *tx)
        .await?;
    require_recovery_on(&mut tx, recoverable).await?;
    tx.commit().await
}

/// Looks up a Role by name.
pub async fn role_by_name(pool: &SqlitePool, name: &str) -> Result<Option<i64>, StoreError> {
    query_scalar::<_, i64>("SELECT id FROM role WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
}

pub(super) async fn validate_role_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<(), StoreError> {
    let exists = query_scalar::<_, i64>("SELECT 1 FROM role WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&mut *connection)
        .await?
        .is_some();
    if !exists {
        return Err(protocol("Person access requires an existing Role"));
    }
    Ok(())
}

/// Lists Roles and their permission keys in name order.
pub async fn list_roles(pool: &SqlitePool) -> Result<Vec<(i64, String, Vec<String>)>, StoreError> {
    let roles = query_as::<_, (i64, String)>("SELECT id, name FROM role ORDER BY name")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(roles.len());
    for (id, name) in roles {
        let permissions = role_permission_keys(pool, &name).await?;
        out.push((id, name, permissions));
    }
    Ok(out)
}

/// Reports whether an eligible recovery Person exists.
pub async fn admin_exists(pool: &SqlitePool) -> Result<bool, StoreError> {
    Ok(!admins(pool).await?.is_empty())
}

/// Lists People eligible for administrative recovery.
pub async fn admins(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    let mut connection = pool.acquire().await?;
    recovery_people_on(&mut connection).await
}

/// Lists permission keys of the named Role in subject and action order.
pub async fn role_permission_keys(
    pool: &SqlitePool,
    role: &str,
) -> Result<Vec<String>, StoreError> {
    query_scalar::<_, String>(
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

/// Reads bounded, validated permission keys for one Role.
pub async fn role_permission_keys_by_id(
    pool: &SqlitePool,
    role_id: i64,
) -> Result<Vec<String>, StoreError> {
    let mut connection = pool.acquire().await?;
    role_permission_keys_by_id_on(&mut connection, role_id).await
}

/// Reads a Role's complete bounded permission set.
///
/// # Errors
/// Rejects missing Roles, invalid stored rows, and oversized permission sets.
pub async fn role_permission_keys_by_id_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<Vec<String>, StoreError> {
    if role_id <= 0 {
        return Err(protocol("invalid Role identity"));
    }
    validate_role_on(connection, role_id).await?;
    let rows = query_as::<_, (i64, i64, i64, i64, Option<String>)>(
        "WITH permissions AS (
             SELECT CASE WHEN typeof(p.subject) = 'text' AND typeof(p.action) = 'text'
                              AND length(trim(p.subject)) > 0 AND length(trim(p.action)) > 0
                              AND typeof(rp.permission_id) = 'integer' AND rp.permission_id > 0
                         THEN 1 ELSE 0 END AS valid,
                    length(CAST(p.subject AS BLOB)) + 1 + length(CAST(p.action AS BLOB)) AS bytes
               FROM role_permission rp LEFT JOIN permission p ON p.id = rp.permission_id
              WHERE rp.role_id = ?
         ), bounds AS (
             SELECT COUNT(*) AS count, COALESCE(SUM(bytes), 0) AS bytes,
                    COALESCE(MAX(bytes), 0) AS largest,
                    COALESCE(SUM(CASE WHEN valid = 1 THEN 0 ELSE 1 END), 0) AS invalid
               FROM permissions
         )
         SELECT b.count, b.bytes, b.largest, b.invalid,
                p.subject || ':' || p.action
           FROM bounds b
           LEFT JOIN role_permission rp ON rp.role_id = ? AND b.count <= ?
                AND b.bytes <= ? AND b.largest <= ? AND b.invalid = 0
           LEFT JOIN permission p ON p.id = rp.permission_id
          ORDER BY p.subject, p.action LIMIT ?",
    )
    .bind(role_id)
    .bind(role_id)
    .bind(MAX_ROLE_PERMISSIONS as i64)
    .bind(MAX_ROLE_PERMISSION_BYTES as i64)
    .bind(MAX_PERMISSION_KEY_BYTES as i64)
    .bind(MAX_ROLE_PERMISSIONS as i64 + 1)
    .fetch_all(&mut *connection)
    .await?;
    let Some(&(count, bytes, largest, invalid, _)) = rows.first() else {
        return Err(protocol("Role permission bounds are unavailable"));
    };
    if !(0..=MAX_ROLE_PERMISSIONS as i64).contains(&count)
        || !(0..=MAX_ROLE_PERMISSION_BYTES as i64).contains(&bytes)
        || !(0..=MAX_PERMISSION_KEY_BYTES as i64).contains(&largest)
        || invalid != 0
    {
        return Err(protocol(
            "Role permissions have invalid or oversized stored data",
        ));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    if usize::try_from(count).ok() != Some(rows.len()) {
        return Err(protocol("Role permission read is incomplete"));
    }
    rows.into_iter()
        .map(|(_, _, _, _, key)| key.ok_or_else(|| protocol("Role permission key is unavailable")))
        .collect()
}
