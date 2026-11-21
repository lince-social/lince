mod retained;
mod write;

pub use retained::{RetainedPersonAccess, retained_person_access_on};
pub(super) use write::validate_person_on;
pub use write::{
    compare_and_set_read_filter, compare_and_set_read_filter_on, compare_and_set_role,
    compare_and_set_role_on, set_user_role,
};

use nucleus::{RecordKind, valid_uid};
use sqlx::{SqliteConnection, SqlitePool, query_as};

use super::protocol;
use crate::StoreError;

/// Maximum byte length of a Person read filter.
pub const MAX_READ_FILTER_BYTES: usize = 1024 * 1024;
/// Maximum number of rows in a retained Person access snapshot.
pub const MAX_PERSON_ACCESS_ROWS: usize = 4096;
/// Maximum total recorded bytes in a Person access snapshot.
pub const MAX_PERSON_ACCESS_BYTES: usize = 4 * 1024 * 1024;
/// An active Person's assigned Role, read filter, and current access revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonAccess {
    pub person_uid: String,
    pub role_id: Option<i64>,
    pub read_filter: Option<String>,
    pub revision: i64,
}

/// Reads the access row of an active Person.
pub async fn person_access(
    pool: &SqlitePool,
    person_uid: &str,
) -> Result<Option<PersonAccess>, StoreError> {
    let mut connection = pool.acquire().await?;
    person_access_on(&mut connection, person_uid).await
}

/// Reads bounded access for an active Person on the supplied connection.
///
/// # Errors
/// Rejects noncanonical Person IDs, corrupt stored rows, and oversized filters.
pub async fn person_access_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<Option<PersonAccess>, StoreError> {
    if !valid_uid(person_uid, "r") {
        return Err(protocol("invalid Person access identity"));
    }
    let row = query_as::<
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
        return Err(protocol(
            "Person access has invalid or oversized stored data",
        ));
    };
    let kind = kind
        .as_deref()
        .and_then(RecordKind::parse)
        .ok_or_else(|| protocol("Person access has an invalid Record kind"))?;
    if kind != RecordKind::Person || present == 0 {
        return Ok(None);
    }
    Ok(Some(PersonAccess {
        person_uid: person_uid.to_string(),
        role_id,
        read_filter,
        revision,
    }))
}
