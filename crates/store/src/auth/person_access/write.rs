use sqlx::{SqliteConnection, SqlitePool, query, query_scalar};

use super::{MAX_READ_FILTER_BYTES, PersonAccess, person_access_on};
use crate::{
    StoreError,
    auth::{protocol, roles::validate_role_on},
    write_tx,
};

pub(in crate::auth) async fn validate_person_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<(), StoreError> {
    let exists = query_scalar::<_, i64>(
        "SELECT 1 FROM record
          WHERE uid = ? AND kind = 'person' AND deleted_at IS NULL",
    )
    .bind(person_uid)
    .fetch_optional(&mut *connection)
    .await?
    .is_some();
    if !exists {
        return Err(protocol(
            "Person access requires an existing undeleted Person",
        ));
    }
    Ok(())
}

fn validate_expected_revision(expected_revision: i64) -> Result<(), StoreError> {
    if expected_revision < 0 {
        return Err(protocol(
            "Person access expected revision cannot be negative",
        ));
    }
    Ok(())
}

fn revision_conflict(expected_revision: i64) -> StoreError {
    protocol(format!(
        "Person access revision conflict at expected revision {expected_revision}"
    ))
}

/// Replaces a Person's assigned Role at the expected access revision.
pub async fn compare_and_set_role(
    pool: &SqlitePool,
    person_uid: &str,
    role_id: Option<i64>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    let mut tx = write_tx(pool).await?;
    let access = compare_and_set_role_on(&mut tx, person_uid, role_id, expected_revision).await?;
    tx.commit().await?;
    Ok(access)
}

/// Changes a Role on the supplied connection. A zero revision expects no access row.
///
/// # Errors
/// Rejects missing People or Roles, a stale revision, and failed database writes.
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
        query(
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
        query(
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
        .ok_or(StoreError::RowNotFound)
}

/// Replaces a Person's read filter at the expected access revision.
pub async fn compare_and_set_read_filter(
    pool: &SqlitePool,
    person_uid: &str,
    read_filter: Option<&str>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    let mut tx = write_tx(pool).await?;
    let access =
        compare_and_set_read_filter_on(&mut tx, person_uid, read_filter, expected_revision).await?;
    tx.commit().await?;
    Ok(access)
}

/// Changes the read filter on the supplied connection. A zero revision expects no row.
///
/// # Errors
/// Rejects oversized filters, invalid People, stale revisions, and failed writes.
pub async fn compare_and_set_read_filter_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    read_filter: Option<&str>,
    expected_revision: i64,
) -> Result<PersonAccess, StoreError> {
    if read_filter.is_some_and(|filter| filter.len() > MAX_READ_FILTER_BYTES) {
        return Err(protocol("Person read filter exceeds its byte limit"));
    }
    validate_expected_revision(expected_revision)?;
    validate_person_on(connection, person_uid).await?;
    let changed = if expected_revision == 0 {
        query(
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
        query(
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
        .ok_or(StoreError::RowNotFound)
}

/// Updates a Person's Role using its current access revision.
pub async fn set_user_role(
    pool: &SqlitePool,
    person_uid: &str,
    role_id: i64,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    let expected = person_access_on(&mut tx, person_uid)
        .await?
        .map_or(0, |access| access.revision);
    compare_and_set_role_on(&mut tx, person_uid, Some(role_id), expected).await?;
    tx.commit().await
}
