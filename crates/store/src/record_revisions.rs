use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRevision {
    pub record_uid: String,
    pub revision: i64,
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn validate_record_uid(record_uid: &str) -> Result<(), StoreError> {
    if !nucleus::valid_uid(record_uid, "r") {
        return Err(protocol("Record revision requires a canonical Record uid"));
    }
    Ok(())
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    record_uid: &str,
) -> Result<RecordRevision, StoreError> {
    validate_record_uid(record_uid)?;
    let row = sqlx::query_as::<_, (i64, String, Option<i64>)>(
        "SELECT rr.record_uid IS NOT NULL,
                typeof(rr.revision),
                CASE WHEN typeof(rr.revision) = 'integer' THEN rr.revision END
           FROM record AS r
           LEFT JOIN record_revision AS rr ON rr.record_uid = r.uid
          WHERE r.uid = ?",
    )
    .bind(record_uid)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| protocol("Record revision requires an existing Record"))?;
    if row.0 == 0 {
        return Err(protocol("Record is missing its revision"));
    }
    if row.1 != "integer" {
        return Err(protocol("Record revision has an invalid storage type"));
    }
    let revision = row
        .2
        .ok_or_else(|| protocol("Record revision is unreadable"))?;
    if revision <= 0 {
        return Err(protocol("Record revision must be positive"));
    }
    Ok(RecordRevision {
        record_uid: record_uid.to_owned(),
        revision,
    })
}

pub async fn check_expected_on(
    connection: &mut SqliteConnection,
    record_uid: &str,
    expected_revision: i64,
) -> Result<RecordRevision, StoreError> {
    if expected_revision <= 0 {
        return Err(protocol(
            "Expected Record revision must be a positive integer",
        ));
    }
    let revision = get_on(connection, record_uid).await?;
    if revision.revision != expected_revision {
        return Err(protocol(format!(
            "Record revision conflict at expected revision {expected_revision}"
        )));
    }
    Ok(revision)
}

/// Observe whether captured revisions still describe live Records. This is a
/// read-only display comparison, not a replacement for guarded-write checks.
/// Missing revision rows and deleted Records compare unequal. Decode in requested
/// order so an earlier mismatch still precedes a later malformed revision.
pub async fn live_match(pool: &SqlitePool, expected: &[(String, i64)]) -> Result<bool, StoreError> {
    let _read = nucleus::performance::Span::new("record-revision-observation-await");
    for expected in expected.chunks(512) {
        let mut query =
            sqlx::QueryBuilder::<Sqlite>::new("WITH requested(position, uid) AS (VALUES ");
        for (position, (uid, _)) in expected.iter().enumerate() {
            if position != 0 {
                query.push(", ");
            }
            query
                .push("(")
                .push(position)
                .push(", ")
                .push_bind(uid)
                .push(")");
        }
        query.push(
            ") SELECT rr.revision, rr.record_uid IS NOT NULL AS present,
                      r.deleted_at IS NOT NULL AS deleted
               FROM requested
               LEFT JOIN record r ON r.uid = requested.uid
               LEFT JOIN record_revision rr ON rr.record_uid = r.uid
              ORDER BY requested.position",
        );
        let rows = query.build().fetch_all(pool).await?;
        nucleus::performance::add("record-revision-observation-batches", 1);
        nucleus::performance::add("record-revision-observation-rows", rows.len() as u64);
        if rows.len() != expected.len() {
            return Err(protocol(
                "Live revision observation returned an invalid row count",
            ));
        }
        for (row, (_, revision)) in rows.iter().zip(expected) {
            if !row.try_get::<bool, _>("present")? {
                return Ok(false);
            }
            let current = row.try_get::<i64, _>("revision")?;
            if current != *revision || row.try_get::<bool, _>("deleted")? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}
