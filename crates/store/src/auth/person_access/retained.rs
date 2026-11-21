use nucleus::valid_uid;
use sqlx::{Sqlite, Transaction, query_as};

use super::{MAX_PERSON_ACCESS_BYTES, MAX_PERSON_ACCESS_ROWS, MAX_READ_FILTER_BYTES};
use crate::{StoreError, auth::protocol};

/// Person access retained for recovery, including deletion status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedPersonAccess {
    pub person_uid: String,
    pub role_id: Option<i64>,
    pub read_filter: Option<String>,
    pub revision: i64,
    pub deleted: bool,
}

/// Reads and validates all retained access rows in the caller's transaction.
///
/// # Errors
/// Rejects corrupt or incomplete snapshots, invalid references, and exceeded limits.
pub async fn retained_person_access_on(
    transaction: &mut Transaction<'_, Sqlite>,
) -> Result<Vec<RetainedPersonAccess>, StoreError> {
    let connection = &mut **transaction;
    let (count, bytes, largest, invalid) = query_as::<_, (i64, i64, i64, i64)>(
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
    if !(0..=MAX_PERSON_ACCESS_ROWS as i64).contains(&count)
        || !(0..=MAX_PERSON_ACCESS_BYTES as i64).contains(&bytes)
        || !(0..=MAX_READ_FILTER_BYTES as i64).contains(&largest)
        || invalid != 0
    {
        return Err(protocol(
            "Retained Person access has invalid or oversized stored data",
        ));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let identities = query_as::<_, (Option<String>, Option<i64>, Option<i64>, i64, i64)>(
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
    if usize::try_from(count).ok() != Some(identities.len()) {
        return Err(protocol(
            "Retained Person access identity read is incomplete",
        ));
    }
    for (person_uid, _, revision, _, valid) in &identities {
        let person_uid = person_uid
            .as_deref()
            .ok_or_else(|| protocol("Retained Person access identity is unavailable"))?;
        if !valid_uid(person_uid, "r") || revision.is_none() || *valid != 1 {
            return Err(protocol(
                "Retained Person access identity or revision is invalid",
            ));
        }
    }
    let filters = query_as::<_, (String, i64, Option<String>)>(
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
        return Err(protocol("Retained Person access filter read is incomplete"));
    }
    identities
        .into_iter()
        .zip(filters)
        .map(
            |(
                (person_uid, role_id, revision, deleted, identity_valid),
                (filter_person_uid, filter_valid, read_filter),
            )| {
                let person_uid = person_uid
                    .ok_or_else(|| protocol("Retained Person access identity is unavailable"))?;
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
                        protocol("Retained Person access revision is unavailable")
                    })?,
                    deleted: deleted != 0,
                })
            },
        )
        .collect()
}
