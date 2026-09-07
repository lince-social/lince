use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::StoreError;

pub const ADMIN_ROLE: &str = "admin";
pub const LINCE_ROLE: &str = "lince";

pub const MAX_READ_FILTER_BYTES: usize = 1024 * 1024;
pub const MAX_PERSON_ACCESS_ROWS: usize = 4096;
pub const MAX_PERSON_ACCESS_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ROLE_PERMISSIONS: usize = 1024;
pub const MAX_PERMISSION_KEY_BYTES: usize = 256;
pub const MAX_ROLE_PERMISSION_BYTES: usize = 64 * 1024;

pub async fn ensure_role(pool: &SqlitePool, name: &str) -> Result<i64, StoreError> {
    let mut connection = pool.acquire().await?;
    ensure_role_on(&mut connection, name)
        .await
        .map(|(role_id, _)| role_id)
}

pub async fn ensure_role_on(
    connection: &mut SqliteConnection,
    name: &str,
) -> Result<(i64, bool), StoreError> {
    let inserted = sqlx::query("INSERT OR IGNORE INTO role (name) VALUES (?)")
        .bind(name)
        .execute(&mut *connection)
        .await?
        .rows_affected()
        == 1;
    let role_id = sqlx::query_scalar::<_, i64>("SELECT id FROM role WHERE name = ?")
        .bind(name)
        .fetch_one(&mut *connection)
        .await?;
    Ok((role_id, inserted))
}

pub async fn ensure_permission(
    pool: &SqlitePool,
    subject: &str,
    action: &str,
) -> Result<i64, StoreError> {
    let mut connection = pool.acquire().await?;
    ensure_permission_on(&mut connection, subject, action).await
}

pub async fn ensure_permission_on(
    connection: &mut SqliteConnection,
    subject: &str,
    action: &str,
) -> Result<i64, StoreError> {
    sqlx::query("INSERT OR IGNORE INTO permission (subject, action) VALUES (?, ?)")
        .bind(subject)
        .bind(action)
        .execute(&mut *connection)
        .await?;
    sqlx::query_scalar::<_, i64>("SELECT id FROM permission WHERE subject = ? AND action = ?")
        .bind(subject)
        .bind(action)
        .fetch_one(&mut *connection)
        .await
}

pub async fn grant(pool: &SqlitePool, role_id: i64, permission_id: i64) -> Result<(), StoreError> {
    let mut connection = pool.acquire().await?;
    grant_on(&mut connection, role_id, permission_id).await
}

pub async fn grant_on(
    connection: &mut SqliteConnection,
    role_id: i64,
    permission_id: i64,
) -> Result<(), StoreError> {
    sqlx::query("INSERT OR IGNORE INTO role_permission (role_id, permission_id) VALUES (?, ?)")
        .bind(role_id)
        .bind(permission_id)
        .execute(&mut *connection)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonAccess {
    pub person_uid: String,
    pub role_id: Option<i64>,
    pub read_filter: Option<String>,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedPersonAccess {
    pub person_uid: String,
    pub role_id: Option<i64>,
    pub read_filter: Option<String>,
    pub revision: i64,
    pub deleted: bool,
}

pub async fn retained_person_access_on(
    transaction: &mut Transaction<'_, Sqlite>,
) -> Result<Vec<RetainedPersonAccess>, StoreError> {
    let connection = &mut **transaction;
    let bounds = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT COUNT(*),
                COALESCE(SUM(length(CAST(a.person_uid AS BLOB))
                    + COALESCE(length(CAST(a.read_filter AS BLOB)), 0) + 24), 0),
                COALESCE(MAX(COALESCE(length(CAST(a.read_filter AS BLOB)), 0)), 0),
                COALESCE(SUM(CASE
                    WHEN typeof(a.person_uid) = 'text'
                     AND length(CAST(a.person_uid AS BLOB)) = 28
                     AND typeof(a.revision) = 'integer' AND a.revision > 0
                     AND (a.role_id IS NULL OR (typeof(a.role_id) = 'integer'
                          AND a.role_id > 0 AND role.id IS NOT NULL))
                     AND (a.read_filter IS NULL OR typeof(a.read_filter) = 'text')
                     AND person.uid IS NOT NULL
                     AND typeof(person.kind) = 'text' AND person.kind = 'person'
                     AND (person.deleted_at IS NULL OR typeof(person.deleted_at) = 'text')
                    THEN 0 ELSE 1 END), 0)
           FROM person_access a
           LEFT JOIN record person ON person.uid = a.person_uid
           LEFT JOIN role ON role.id = a.role_id",
    )
    .fetch_one(&mut *connection)
    .await?;
    if bounds.0 < 0
        || bounds.0 > MAX_PERSON_ACCESS_ROWS as i64
        || bounds.1 < 0
        || bounds.1 > MAX_PERSON_ACCESS_BYTES as i64
        || bounds.2 < 0
        || bounds.2 > MAX_READ_FILTER_BYTES as i64
        || bounds.3 != 0
    {
        return Err(sqlx::Error::Protocol(
            "Retained Person access has invalid or oversized stored data".into(),
        ));
    }
    if bounds.0 == 0 {
        return Ok(Vec::new());
    }
    let identities = sqlx::query_as::<_, (Option<String>, Option<i64>, Option<i64>, i64, i64)>(
        "SELECT CASE
                    WHEN typeof(a.person_uid) = 'text'
                     AND length(CAST(a.person_uid AS BLOB)) = 28
                    THEN a.person_uid END,
                CASE
                    WHEN a.role_id IS NULL OR (typeof(a.role_id) = 'integer'
                         AND a.role_id > 0 AND role.id IS NOT NULL)
                    THEN a.role_id END,
                CASE
                    WHEN typeof(a.revision) = 'integer' AND a.revision > 0
                    THEN a.revision END,
                person.deleted_at IS NOT NULL,
                CASE
                    WHEN typeof(a.person_uid) = 'text'
                     AND length(CAST(a.person_uid AS BLOB)) = 28
                     AND typeof(a.revision) = 'integer' AND a.revision > 0
                     AND (a.role_id IS NULL OR (typeof(a.role_id) = 'integer'
                          AND a.role_id > 0 AND role.id IS NOT NULL))
                     AND person.uid IS NOT NULL
                     AND typeof(person.kind) = 'text' AND person.kind = 'person'
                     AND (person.deleted_at IS NULL OR typeof(person.deleted_at) = 'text')
                    THEN 1 ELSE 0 END
           FROM person_access a
           LEFT JOIN record person ON person.uid = a.person_uid
           LEFT JOIN role ON role.id = a.role_id
          ORDER BY a.person_uid
          LIMIT ?",
    )
    .bind(MAX_PERSON_ACCESS_ROWS as i64 + 1)
    .fetch_all(&mut *connection)
    .await?;
    if identities.len() != bounds.0 as usize {
        return Err(sqlx::Error::Protocol(
            "Retained Person access identity read is incomplete".into(),
        ));
    }
    for (person_uid, _, revision, _, valid) in &identities {
        let person_uid = person_uid.as_deref().ok_or_else(|| {
            sqlx::Error::Protocol("Retained Person access identity is unavailable".into())
        })?;
        if !nucleus::valid_uid(person_uid, "r") || revision.is_none() || *valid != 1 {
            return Err(sqlx::Error::Protocol(
                "Retained Person access identity or revision is invalid".into(),
            ));
        }
    }
    let filters = sqlx::query_as::<_, (String, i64, Option<String>)>(
        "SELECT a.person_uid,
                CASE
                    WHEN a.read_filter IS NULL OR (typeof(a.read_filter) = 'text'
                         AND length(CAST(a.read_filter AS BLOB)) <= ?)
                    THEN 1 ELSE 0 END,
                CASE
                    WHEN a.read_filter IS NULL OR (typeof(a.read_filter) = 'text'
                         AND length(CAST(a.read_filter AS BLOB)) <= ?)
                    THEN a.read_filter END
           FROM person_access a
          ORDER BY a.person_uid
          LIMIT ?",
    )
    .bind(MAX_READ_FILTER_BYTES as i64)
    .bind(MAX_READ_FILTER_BYTES as i64)
    .bind(MAX_PERSON_ACCESS_ROWS as i64 + 1)
    .fetch_all(&mut *connection)
    .await?;
    if filters.len() != identities.len() {
        return Err(sqlx::Error::Protocol(
            "Retained Person access filter read is incomplete".into(),
        ));
    }
    identities
        .into_iter()
        .zip(filters)
        .map(
            |(
                (person_uid, role_id, revision, deleted, identity_valid),
                (filter_person_uid, filter_valid, read_filter),
            )| {
                let person_uid = person_uid.ok_or_else(|| {
                    sqlx::Error::Protocol("Retained Person access identity is unavailable".into())
                })?;
                if filter_person_uid != person_uid || identity_valid != 1 || filter_valid != 1 {
                    return Err(sqlx::Error::Protocol(
                        "Retained Person access rows changed or became corrupt during the read"
                            .into(),
                    ));
                }
                Ok(RetainedPersonAccess {
                    person_uid,
                    role_id,
                    read_filter,
                    revision: revision.ok_or_else(|| {
                        sqlx::Error::Protocol(
                            "Retained Person access revision is unavailable".into(),
                        )
                    })?,
                    deleted: deleted != 0,
                })
            },
        )
        .collect()
}

pub async fn person_access(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Option<PersonAccess>, StoreError> {
    let mut connection = pool.acquire().await?;
    person_access_on(&mut connection, person_uid).await
}

pub async fn person_access_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<Option<PersonAccess>, StoreError> {
    if !nucleus::valid_uid(person_uid, "r") {
        return Err(sqlx::Error::Protocol(
            "invalid Person access identity".into(),
        ));
    }
    let row = sqlx::query_as::<
        _,
        (
            i64,
            Option<String>,
            i64,
            Option<i64>,
            Option<String>,
            Option<i64>,
        ),
    >(
        "WITH bounded AS (
             SELECT a.role_id, a.read_filter, a.revision, p.kind, p.deleted_at,
                    CASE WHEN p.uid IS NOT NULL AND typeof(p.kind) = 'text'
                              AND length(CAST(p.kind AS BLOB)) <= 32
                              AND (p.deleted_at IS NULL OR typeof(p.deleted_at) = 'text')
                              AND typeof(a.revision) = 'integer' AND a.revision > 0
                              AND (a.role_id IS NULL OR (typeof(a.role_id) = 'integer'
                                   AND a.role_id > 0 AND r.id IS NOT NULL))
                              AND (a.read_filter IS NULL OR (typeof(a.read_filter) = 'text'
                                   AND length(CAST(a.read_filter AS BLOB)) <= ?))
                         THEN 1 ELSE 0 END AS valid
               FROM person_access a
               LEFT JOIN record p ON p.uid = a.person_uid
               LEFT JOIN role r ON r.id = a.role_id
              WHERE a.person_uid = ?
         )
         SELECT valid, CASE WHEN valid = 1 THEN kind END,
                CASE WHEN deleted_at IS NULL THEN 1 ELSE 0 END,
                CASE WHEN valid = 1 THEN role_id END,
                CASE WHEN valid = 1 AND kind = 'person' AND deleted_at IS NULL THEN read_filter END,
                CASE WHEN valid = 1 THEN revision END
           FROM bounded",
    )
    .bind(MAX_READ_FILTER_BYTES as i64)
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let (1, kind, present, role_id, read_filter, Some(revision)) = row else {
        return Err(sqlx::Error::Protocol(
            "Person access has invalid or oversized stored data".into(),
        ));
    };
    let kind = kind
        .as_deref()
        .and_then(nucleus::RecordKind::parse)
        .ok_or_else(|| sqlx::Error::Protocol("Person access has an invalid Record kind".into()))?;
    if kind != nucleus::RecordKind::Person || present == 0 {
        return Ok(None);
    }
    Ok(Some(PersonAccess {
        person_uid: person_uid.to_string(),
        role_id,
        read_filter,
        revision,
    }))
}

async fn validate_person_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<(), StoreError> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM record
          WHERE uid = ? AND kind = 'person' AND deleted_at IS NULL",
    )
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?
    .is_some();
    if !exists {
        return Err(sqlx::Error::Protocol(
            "Person access requires an existing undeleted Person".into(),
        ));
    }
    Ok(())
}

async fn validate_role_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<(), StoreError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM role WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&mut *connection)
        .await?
        .is_some();
    if !exists {
        return Err(sqlx::Error::Protocol(
            "Person access requires an existing Role".into(),
        ));
    }
    Ok(())
}

fn validate_expected_revision(expected_revision: i64) -> Result<(), StoreError> {
    if expected_revision < 0 {
        return Err(sqlx::Error::Protocol(
            "Person access expected revision cannot be negative".into(),
        ));
    }
    Ok(())
}

fn revision_conflict(expected_revision: i64) -> StoreError {
    sqlx::Error::Protocol(format!(
        "Person access revision conflict at expected revision {expected_revision}"
    ))
}

pub async fn compare_and_set_role(
    pool: &SqlitePool,
    person_uid: &str,
    role_id: Option<i64>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let access = compare_and_set_role_on(&mut tx, person_uid, role_id, expected_revision).await?;
    tx.commit().await?;
    Ok(access)
}

pub async fn compare_and_set_role_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    role_id: Option<i64>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    validate_expected_revision(expected_revision)?;
    validate_person_on(connection, person_uid).await?;
    if let Some(role_id) = role_id {
        validate_role_on(connection, role_id).await?;
    }
    let changed = if expected_revision == 0 {
        sqlx::query(
            "INSERT INTO person_access (person_uid, role_id, revision)
             VALUES (?, ?, 1)
             ON CONFLICT(person_uid) DO NOTHING",
        )
        .bind(person_uid)
        .bind(role_id)
        .execute(&mut *connection)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "UPDATE person_access
                SET role_id = ?, revision = revision + 1
              WHERE person_uid = ? AND revision = ?",
        )
        .bind(role_id)
        .bind(person_uid)
        .bind(expected_revision)
        .execute(&mut *connection)
        .await?
        .rows_affected()
    };
    if changed != 1 {
        return Err(revision_conflict(expected_revision));
    }
    person_access_on(connection, person_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn compare_and_set_read_filter(
    pool: &SqlitePool,
    person_uid: &str,
    read_filter: Option<&str>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let access =
        compare_and_set_read_filter_on(&mut tx, person_uid, read_filter, expected_revision).await?;
    tx.commit().await?;
    Ok(access)
}

pub async fn compare_and_set_read_filter_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    read_filter: Option<&str>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    if read_filter.is_some_and(|filter| filter.len() > MAX_READ_FILTER_BYTES) {
        return Err(sqlx::Error::Protocol(
            "Person read filter exceeds its byte limit".into(),
        ));
    }
    validate_expected_revision(expected_revision)?;
    validate_person_on(connection, person_uid).await?;
    let changed = if expected_revision == 0 {
        sqlx::query(
            "INSERT INTO person_access (person_uid, read_filter, revision)
             VALUES (?, ?, 1)
             ON CONFLICT(person_uid) DO NOTHING",
        )
        .bind(person_uid)
        .bind(read_filter)
        .execute(&mut *connection)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "UPDATE person_access
                SET read_filter = ?, revision = revision + 1
              WHERE person_uid = ? AND revision = ?",
        )
        .bind(read_filter)
        .bind(person_uid)
        .bind(expected_revision)
        .execute(&mut *connection)
        .await?
        .rows_affected()
    };
    if changed != 1 {
        return Err(revision_conflict(expected_revision));
    }
    person_access_on(connection, person_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn set_user_role(
    pool: &SqlitePool,
    person_uid: &str,
    role_id: i64,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let expected = person_access_on(&mut tx, person_uid)
        .await?
        .map_or(0, |access| access.revision);
    compare_and_set_role_on(&mut tx, person_uid, Some(role_id), expected).await?;
    tx.commit().await?;
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
              AND p.kind = 'person' AND p.deleted_at IS NULL
         LEFT JOIN person_access a ON a.person_uid = c.person_uid
         LEFT JOIN role r ON r.id = a.role_id
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
        "SELECT COUNT(1) FROM person_access a
         JOIN record p ON p.uid = a.person_uid
         WHERE a.role_id = (SELECT id FROM role WHERE name = ?)
           AND p.kind = 'person' AND p.deleted_at IS NULL",
    )
    .bind(ADMIN_ROLE)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn admins(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar::<_, String>(
        "SELECT a.person_uid FROM person_access a
         JOIN record p ON p.uid = a.person_uid
         WHERE a.role_id = (SELECT id FROM role WHERE name = ?)
           AND p.kind = 'person' AND p.deleted_at IS NULL",
    )
    .bind(ADMIN_ROLE)
    .fetch_all(pool)
    .await
}

fn validate_credential_input(username: &str, password_hash: &str) -> Result<(), StoreError> {
    if username.trim().is_empty() || username.len() > crate::session_access::MAX_USERNAME_BYTES {
        return Err(sqlx::Error::Protocol(
            "Credential username is empty or exceeds its byte limit".into(),
        ));
    }
    if password_hash.trim().is_empty()
        || password_hash.len() > crate::session_access::MAX_PASSWORD_HASH_BYTES
    {
        return Err(sqlx::Error::Protocol(
            "Credential hash is empty or exceeds its byte limit".into(),
        ));
    }
    Ok(())
}

pub async fn credential_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<i64, StoreError> {
    if !nucleus::valid_uid(person_uid, "r") {
        return Err(sqlx::Error::Protocol(
            "Credential requires a canonical Person identity".into(),
        ));
    }
    let row = sqlx::query_as::<_, (Option<String>, i64, Option<i64>)>(
        "SELECT CASE
                    WHEN typeof(person.kind) = 'text'
                     AND length(CAST(person.kind AS BLOB)) <= 32
                    THEN person.kind END,
                person.deleted_at IS NULL,
                CASE
                    WHEN typeof(generation.generation) = 'integer'
                     AND generation.generation > 0
                    THEN generation.generation END
           FROM record person
           LEFT JOIN person_auth_generation generation
             ON generation.person_uid = person.uid
          WHERE person.uid = ?",
    )
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("Credential Person is missing".into()))?;
    if row.0.as_deref() != Some("person") {
        return Err(sqlx::Error::Protocol(
            "Credential target is not a valid Person".into(),
        ));
    }
    if row.1 == 0 {
        return Err(sqlx::Error::Protocol("Credential Person is deleted".into()));
    }
    let generation = row.2.ok_or_else(|| {
        sqlx::Error::Protocol("Credential authentication generation is missing or corrupt".into())
    })?;
    Ok(generation)
}

async fn require_credential_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    if expected_generation <= 0 {
        return Err(sqlx::Error::Protocol(
            "Credential expected generation must be positive".into(),
        ));
    }
    let generation = credential_generation_on(connection, person_uid).await?;
    if generation != expected_generation {
        return Err(sqlx::Error::Protocol(
            "Credential authentication generation conflict".into(),
        ));
    }
    if generation == i64::MAX {
        return Err(sqlx::Error::Protocol(
            "Credential authentication generation is exhausted".into(),
        ));
    }
    Ok(generation)
}

async fn credential_present_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<bool, StoreError> {
    let valid = sqlx::query_scalar::<_, i64>(
        "SELECT CASE
                    WHEN typeof(username) = 'text' AND length(trim(username)) > 0
                     AND length(CAST(username AS BLOB)) <= ?
                     AND typeof(password_hash) = 'text' AND length(trim(password_hash)) > 0
                     AND length(CAST(password_hash AS BLOB)) <= ?
                    THEN 1 ELSE 0 END
           FROM person_credential
          WHERE person_uid = ?",
    )
    .bind(crate::session_access::MAX_USERNAME_BYTES as i64)
    .bind(crate::session_access::MAX_PASSWORD_HASH_BYTES as i64)
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?;
    match valid {
        None => Ok(false),
        Some(1) => Ok(true),
        Some(_) => Err(sqlx::Error::Protocol(
            "Credential has invalid or oversized stored data".into(),
        )),
    }
}

async fn advanced_credential_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    previous_generation: i64,
) -> Result<i64, StoreError> {
    let generation = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT CASE
                    WHEN typeof(generation) = 'integer' AND generation > 0
                    THEN generation END
           FROM person_auth_generation
          WHERE person_uid = ?",
    )
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?
    .flatten()
    .ok_or_else(|| {
        sqlx::Error::Protocol("Credential authentication generation is missing or corrupt".into())
    })?;
    if generation <= previous_generation {
        return Err(sqlx::Error::Protocol(
            "Credential authentication generation did not advance".into(),
        ));
    }
    Ok(generation)
}

pub async fn create_credential_on(
    transaction: &mut Transaction<'_, Sqlite>,
    person_uid: &str,
    username: &str,
    password_hash: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    let connection = &mut **transaction;
    validate_credential_input(username, password_hash)?;
    let generation =
        require_credential_generation_on(connection, person_uid, expected_generation).await?;
    if credential_present_on(connection, person_uid).await? {
        return Err(sqlx::Error::Protocol("Credential already exists".into()));
    }
    sqlx::query(
        "INSERT INTO person_credential (person_uid, username, password_hash)
         VALUES (?, ?, ?)",
    )
    .bind(person_uid)
    .bind(username)
    .bind(password_hash)
    .execute(&mut *connection)
    .await?;
    advanced_credential_generation_on(connection, person_uid, generation).await
}

pub async fn replace_credential_on(
    transaction: &mut Transaction<'_, Sqlite>,
    person_uid: &str,
    username: &str,
    password_hash: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    let connection = &mut **transaction;
    validate_credential_input(username, password_hash)?;
    let generation =
        require_credential_generation_on(connection, person_uid, expected_generation).await?;
    if !credential_present_on(connection, person_uid).await? {
        return Err(sqlx::Error::Protocol("Credential is missing".into()));
    }
    let changed = sqlx::query(
        "UPDATE person_credential
            SET username = ?, password_hash = ?, updated_at = CURRENT_TIMESTAMP
          WHERE person_uid = ?",
    )
    .bind(username)
    .bind(password_hash)
    .bind(person_uid)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(sqlx::Error::Protocol("Credential is missing".into()));
    }
    advanced_credential_generation_on(connection, person_uid, generation).await
}

pub async fn remove_credential_on(
    transaction: &mut Transaction<'_, Sqlite>,
    person_uid: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    let connection = &mut **transaction;
    let generation =
        require_credential_generation_on(connection, person_uid, expected_generation).await?;
    if !credential_present_on(connection, person_uid).await? {
        return Err(sqlx::Error::Protocol("Credential is missing".into()));
    }
    let changed = sqlx::query("DELETE FROM person_credential WHERE person_uid = ?")
        .bind(person_uid)
        .execute(&mut *connection)
        .await?
        .rows_affected();
    if changed != 1 {
        return Err(sqlx::Error::Protocol("Credential is missing".into()));
    }
    advanced_credential_generation_on(connection, person_uid, generation).await
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
    let mut tx = crate::write_tx(pool).await?;
    validate_person_on(&mut tx, person_uid).await?;
    if person_access_on(&mut tx, person_uid).await?.is_none() {
        validate_role_on(&mut tx, role_id).await?;
        sqlx::query("INSERT INTO person_access (person_uid, role_id, revision) VALUES (?, ?, 1)")
            .bind(person_uid)
            .bind(role_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO person_credential (person_uid, username, password_hash)
         VALUES (?, ?, ?)
         ON CONFLICT(person_uid) DO UPDATE SET
             username = excluded.username,
             password_hash = excluded.password_hash,
             updated_at = CURRENT_TIMESTAMP",
    )
    .bind(person_uid)
    .bind(username)
    .bind(password_hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
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

pub async fn role_permission_keys_by_id(
    pool: &SqlitePool,
    role_id: i64,
) -> Result<Vec<String>, StoreError> {
    let mut connection = pool.acquire().await?;
    role_permission_keys_by_id_on(&mut connection, role_id).await
}

pub async fn role_permission_keys_by_id_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<Vec<String>, StoreError> {
    if role_id <= 0 {
        return Err(sqlx::Error::Protocol("invalid Role identity".into()));
    }
    validate_role_on(connection, role_id).await?;
    let rows = sqlx::query_as::<_, (i64, i64, i64, i64, Option<String>)>(
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
    let Some((count, bytes, largest, invalid, _)) = rows.first() else {
        return Err(sqlx::Error::Protocol(
            "Role permission bounds are unavailable".into(),
        ));
    };
    if *count < 0
        || *count > MAX_ROLE_PERMISSIONS as i64
        || *bytes < 0
        || *bytes > MAX_ROLE_PERMISSION_BYTES as i64
        || *largest < 0
        || *largest > MAX_PERMISSION_KEY_BYTES as i64
        || *invalid != 0
    {
        return Err(sqlx::Error::Protocol(
            "Role permissions have invalid or oversized stored data".into(),
        ));
    }
    if *count == 0 {
        return Ok(Vec::new());
    }
    if rows.len() != *count as usize {
        return Err(sqlx::Error::Protocol(
            "Role permission read is incomplete".into(),
        ));
    }
    rows.into_iter()
        .map(|(_, _, _, _, key)| {
            key.ok_or_else(|| sqlx::Error::Protocol("Role permission key is unavailable".into()))
        })
        .collect()
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
    SELECT c.person_uid, c.username, p.head, c.password_hash, a.role_id, r.name
    FROM person_credential c
    JOIN record p ON p.uid = c.person_uid
         AND p.kind = 'person' AND p.deleted_at IS NULL
    LEFT JOIN person_access a ON a.person_uid = c.person_uid
    LEFT JOIN role r ON r.id = a.role_id
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
    let role_id = role_id.unwrap_or_default();
    let permissions = if role_id == 0 {
        Vec::new()
    } else {
        role_permission_keys_by_id(pool, role_id).await?
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
