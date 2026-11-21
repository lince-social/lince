use nucleus::{RecordKind, execution::now, valid_uid};
use sqlx::{
    Sqlite, SqliteConnection, SqlitePool, Transaction, query, query::Query, query_as, query_scalar,
    sqlite::SqliteArguments,
};

use super::{
    person_access::{person_access_on, validate_person_on},
    protocol,
    roles::validate_role_on,
};
use crate::{
    StoreError,
    exact::zero,
    records::{NewRecord, create},
    session_access::{MAX_PASSWORD_HASH_BYTES, MAX_USERNAME_BYTES},
    write_tx,
};

macro_rules! credential_insert {
    ($suffix:literal) => {
        concat!(
            "INSERT INTO person_credential
                (person_uid, username, password_hash, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
            $suffix,
        )
    };
}

fn insert_credential(
    sql: &'static str,
    person: &str,
    username: &str,
    hash: &str,
) -> Query<'static, Sqlite, SqliteArguments> {
    query(sql)
        .bind(person)
        .bind(username)
        .bind(hash)
        .bind(now().to_rfc3339())
        .bind(now().to_rfc3339())
}

/// Checks whether a retained credential row exists for the Person.
pub async fn has_credential(pool: &SqlitePool, person_uid: &str) -> Result<bool, StoreError> {
    query_scalar("SELECT EXISTS(SELECT 1 FROM person_credential WHERE person_uid = ?)")
        .bind(person_uid)
        .fetch_one(pool)
        .await
}

fn validate_credential_input(username: &str, password_hash: &str) -> Result<(), StoreError> {
    if username.trim().is_empty() || username.len() > MAX_USERNAME_BYTES {
        return Err(protocol(
            "Credential username is empty or exceeds its byte limit",
        ));
    }
    if password_hash.trim().is_empty() || password_hash.len() > MAX_PASSWORD_HASH_BYTES {
        return Err(protocol(
            "Credential hash is empty or exceeds its byte limit",
        ));
    }
    Ok(())
}

/// Reads the current authentication generation of an active Person.
///
/// # Errors
/// Rejects invalid identities, missing or deleted People, and corrupt generations.
pub async fn credential_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<i64, StoreError> {
    if !valid_uid(person_uid, "r") {
        return Err(protocol("Credential requires a canonical Person identity"));
    }
    let (kind, live, generation) = query_as::<_, (Option<String>, i64, Option<i64>)>(
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
    .ok_or_else(|| protocol("Credential Person is missing"))?;
    if kind.as_deref() != Some("person") {
        return Err(protocol("Credential target is not a valid Person"));
    }
    if live == 0 {
        return Err(protocol("Credential Person is deleted"));
    }
    generation.ok_or_else(missing_generation)
}

fn missing_generation() -> StoreError {
    protocol("Credential authentication generation is missing or corrupt")
}

async fn require_credential_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    if expected_generation <= 0 {
        return Err(protocol("Credential expected generation must be positive"));
    }
    let generation = credential_generation_on(connection, person_uid).await?;
    if generation != expected_generation {
        return Err(protocol("Credential authentication generation conflict"));
    }
    if generation == i64::MAX {
        return Err(protocol(
            "Credential authentication generation is exhausted",
        ));
    }
    Ok(generation)
}

async fn credential_present_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<bool, StoreError> {
    let valid = query_scalar::<_, i64>(
        "SELECT CASE
                    WHEN typeof(username) = 'text' AND length(trim(username)) > 0
                     AND length(CAST(username AS BLOB)) <= ?
                     AND typeof(password_hash) = 'text' AND length(trim(password_hash)) > 0
                     AND length(CAST(password_hash AS BLOB)) <= ?
                    THEN 1 ELSE 0 END
           FROM person_credential
          WHERE person_uid = ?",
    )
    .bind(MAX_USERNAME_BYTES as i64)
    .bind(MAX_PASSWORD_HASH_BYTES as i64)
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?;
    match valid {
        None => Ok(false),
        Some(1) => Ok(true),
        Some(_) => Err(protocol("Credential has invalid or oversized stored data")),
    }
}

async fn existing_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    expected: i64,
) -> Result<i64, StoreError> {
    let generation = require_credential_generation_on(connection, person_uid, expected).await?;
    if !credential_present_on(connection, person_uid).await? {
        return Err(protocol("Credential is missing"));
    }
    Ok(generation)
}

async fn advanced_credential_generation_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    previous_generation: i64,
) -> Result<i64, StoreError> {
    let generation = query_scalar::<_, Option<i64>>(
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
    .ok_or_else(missing_generation)?;
    if generation <= previous_generation {
        return Err(protocol(
            "Credential authentication generation did not advance",
        ));
    }
    Ok(generation)
}

/// Creates a credential at the expected generation in the caller's transaction.
///
/// # Errors
/// Rejects invalid input, an existing credential, exhausted or stale generations,
/// and failed database writes.
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
        return Err(protocol("Credential already exists"));
    }
    insert_credential(credential_insert!(""), person_uid, username, password_hash)
        .execute(&mut *connection)
        .await?;
    advanced_credential_generation_on(connection, person_uid, generation).await
}

/// Replaces an existing credential at the expected generation.
///
/// # Errors
/// Rejects invalid input, corrupt or missing credentials, stale generations,
/// and failed database writes.
pub async fn replace_credential_on(
    transaction: &mut Transaction<'_, Sqlite>,
    person_uid: &str,
    username: &str,
    password_hash: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    let connection = &mut **transaction;
    validate_credential_input(username, password_hash)?;
    let generation = existing_generation_on(connection, person_uid, expected_generation).await?;
    let changed = query(
        "UPDATE person_credential
            SET username = ?, password_hash = ?, updated_at = ?
          WHERE person_uid = ?",
    )
    .bind(username)
    .bind(password_hash)
    .bind(now().to_rfc3339())
    .bind(person_uid)
    .execute(&mut *connection)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(protocol("Credential is missing"));
    }
    advanced_credential_generation_on(connection, person_uid, generation).await
}

/// Removes an existing credential and returns the advanced generation.
///
/// # Errors
/// Rejects corrupt or missing credentials, stale or exhausted generations,
/// and failed database writes.
pub async fn remove_credential_on(
    transaction: &mut Transaction<'_, Sqlite>,
    person_uid: &str,
    expected_generation: i64,
) -> Result<i64, StoreError> {
    let connection = &mut **transaction;
    let generation = existing_generation_on(connection, person_uid, expected_generation).await?;
    let changed = query("DELETE FROM person_credential WHERE person_uid = ?")
        .bind(person_uid)
        .execute(&mut *connection)
        .await?
        .rows_affected();
    if changed != 1 {
        return Err(protocol("Credential is missing"));
    }
    advanced_credential_generation_on(connection, person_uid, generation).await
}

/// Creates a Person and saves its initial login and Role.
pub async fn create_person_login(
    pool: &SqlitePool,
    name: &str,
    username: &str,
    password_hash: &str,
    role_id: i64,
) -> Result<String, StoreError> {
    let person = create(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: name,
            body: "",
            quantity: zero(),
        },
    )
    .await?;
    create_credential(pool, &person.uid, username, password_hash, role_id).await
}

/// Creates or replaces a login while preserving existing Person access settings.
pub async fn create_credential(
    pool: &SqlitePool,
    person_uid: &str,
    username: &str,
    password_hash: &str,
    role_id: i64,
) -> Result<String, StoreError> {
    let mut tx = write_tx(pool).await?;
    validate_person_on(&mut tx, person_uid).await?;
    if person_access_on(&mut tx, person_uid).await?.is_none() {
        validate_role_on(&mut tx, role_id).await?;
        query("INSERT INTO person_access (person_uid, role_id, revision) VALUES (?, ?, 1)")
            .bind(person_uid)
            .bind(role_id)
            .execute(&mut *tx)
            .await?;
    }
    insert_credential(
        credential_insert!(
            " ON CONFLICT(person_uid) DO UPDATE SET
             username = excluded.username,
             password_hash = excluded.password_hash,
             updated_at = excluded.updated_at"
        ),
        person_uid,
        username,
        password_hash,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(person_uid.to_string())
}
