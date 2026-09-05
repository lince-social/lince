use sqlx::SqlitePool;

use crate::StoreError;

pub const ADMIN_ROLE: &str = "admin";
pub const LINCE_ROLE: &str = "lince";

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

pub async fn grant(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    sqlx::query("INSERT OR IGNORE INTO role_permission (role_id, permission_id) VALUES (?, ?)")
        .bind(role_id)
        .bind(permission_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn revoke(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM role_permission WHERE role_id = ? AND permission_id = ?")
        .bind(role_id)
        .bind(permission_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn role_by_name(pool: &SqlitePool, name: &str) -> Result<Option<i64>, StoreError> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM role WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await
}

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

pub async fn has_credential(pool: &SqlitePool, person_uid: &str) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM person_credential WHERE person_uid = ?")
            .bind(person_uid)
            .fetch_optional(pool)
            .await?
            .is_some(),
    )
}

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

pub async fn admins(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar::<_, String>(
        "SELECT person_uid FROM person_credential
         WHERE role_id = (SELECT id FROM role WHERE name = ?)",
    )
    .bind(ADMIN_ROLE)
    .fetch_all(pool)
    .await
}

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
    credential_row(pool, "c.username", username).await
}

pub async fn user_by_uid(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Option<AuthUser>, StoreError> {
    credential_row(pool, "c.person_uid", person_uid).await
}
