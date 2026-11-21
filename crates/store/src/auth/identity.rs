use std::fmt::{self, Debug};

use sqlx::{SqliteConnection, SqlitePool, query_as, query_scalar};

use super::{person_access_on, protocol};
use crate::{StoreError, people::is_active_on, person_roles::permissions_on};

/// Lists active People with credentials in username order.
pub async fn list_users(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String, String)>, StoreError> {
    query_as(
        "SELECT c.person_uid, c.username, p.head, COALESCE(r.name, '')
         FROM person_credential c
         JOIN record p ON p.uid = c.person_uid
              AND p.kind = 'person' AND p.deleted_at IS NULL
         LEFT JOIN person_access a ON a.person_uid = c.person_uid
         LEFT JOIN role r ON r.id = a.role_id
         ORDER BY c.username",
    )
    .fetch_all(pool)
    .await
}

/// Lists the bounded catalogue of active People, including those without logins.
pub async fn list_actors(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String, String)>, StoreError> {
    let rows = query_as::<_, (String, String, String, String)>("SELECT p.uid, COALESCE(c.username, ''), p.head, COALESCE(r.name, '') FROM record p LEFT JOIN person_credential c ON c.person_uid = p.uid LEFT JOIN person_access a ON a.person_uid = p.uid LEFT JOIN role r ON r.id = a.role_id WHERE p.kind = 'person' AND p.deleted_at IS NULL ORDER BY p.head LIMIT 4097")
        .fetch_all(pool).await?;
    if rows.len() > 4096 {
        return Err(protocol("Actor catalogue exceeds its bounds"));
    }
    Ok(rows)
}

/// A stored login and its effective role. Debug output redacts the password hash.
#[derive(Clone, PartialEq)]
pub struct AuthUser {
    pub uid: String,
    pub username: String,
    pub name: String,
    pub password_hash: String,
    pub role_id: i64,
    pub role: String,
    pub permissions: Vec<String>,
}

impl Debug for AuthUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthUser")
            .field("uid", &self.uid)
            .field("username", &self.username)
            .field("name", &self.name)
            .field("password_hash", &"<redacted>")
            .field("role_id", &self.role_id)
            .field("role", &self.role)
            .field("permissions", &self.permissions)
            .finish()
    }
}

impl AuthUser {
    /// Checks whether the user's permission keys include the supplied key.
    pub fn permits(&self, permission: &str) -> bool {
        self.permissions.iter().any(|key| key == permission)
    }
}

/// An active identity with an assigned Role and effective permissions.
#[derive(Debug, Clone)]
pub struct Principal {
    pub uid: String,
    pub role_id: i64,
    pub role: String,
    pub permissions: Vec<String>,
}

impl Principal {
    /// Checks whether the principal has the supplied permission key.
    pub fn permits(&self, permission: &str) -> bool {
        self.permissions.iter().any(|key| key == permission)
    }
}

/// Reads an active Person's principal from one read transaction.
pub async fn principal(pool: &SqlitePool, person: &str) -> Result<Option<Principal>, StoreError> {
    let mut tx = pool.begin().await?;
    let result = principal_on(&mut tx, person).await?;
    tx.commit().await?;
    Ok(result)
}

/// Looks up the principal of a Person with current active standing.
pub async fn principal_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<Option<Principal>, StoreError> {
    if !is_active_on(connection, person).await? {
        return Ok(None);
    }
    assigned_role_on(connection, person).await
}

/// Reads the Person's assigned Role and effective permissions.
pub async fn assigned_role_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<Option<Principal>, StoreError> {
    let Some(access) = person_access_on(connection, person).await? else {
        return Ok(None);
    };
    let Some(role_id) = access.role_id else {
        return Ok(None);
    };
    let role = query_scalar::<_, String>("SELECT name FROM role WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(role) = role else {
        return Ok(None);
    };
    let permissions = permissions_on(connection, person).await?;
    Ok(Some(Principal {
        uid: person.into(),
        role_id,
        role,
        permissions,
    }))
}

macro_rules! credential_select {
    ($column:literal) => {
        concat!(
            "SELECT c.person_uid, c.username, p.head, c.password_hash, a.role_id, r.name
               FROM person_credential c
               JOIN record p ON p.uid = c.person_uid
                    AND p.kind = 'person' AND p.deleted_at IS NULL
               LEFT JOIN person_access a ON a.person_uid = c.person_uid
               LEFT JOIN role r ON r.id = a.role_id
              WHERE ",
            $column,
            " = ?",
        )
    };
}

async fn credential_row(
    pool: &SqlitePool,
    sql: &'static str,
    value: &str,
) -> Result<Option<AuthUser>, StoreError> {
    let Some((uid, username, name, password_hash, role_id, role)) =
        query_as::<_, (String, String, String, String, Option<i64>, Option<String>)>(sql)
            .bind(value)
            .fetch_optional(pool)
            .await?
    else {
        return Ok(None);
    };

    let role = role.unwrap_or_default();
    let role_id = role_id.unwrap_or_default();
    let permissions = if role_id == 0 {
        Vec::new()
    } else {
        permissions_on(&mut *pool.acquire().await?, &uid).await?
    };

    Ok(Some(AuthUser {
        uid,
        username,
        name,
        password_hash,
        role_id,
        role,
        permissions,
    }))
}

/// Looks up the stored login of an active Person by username.
pub async fn user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<AuthUser>, StoreError> {
    credential_row(pool, credential_select!("c.username"), username).await
}

/// Looks up the stored login of an active Person by Record UID.
pub async fn user_by_uid(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Option<AuthUser>, StoreError> {
    credential_row(pool, credential_select!("c.person_uid"), person_uid).await
}
