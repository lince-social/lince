//! Record revision reads and live snapshot comparisons.

use nucleus::{
    performance::{Span, add},
    valid_uid,
};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection, SqlitePool, query_as};

use crate::StoreError;

const BATCH_SIZE: usize = 512;

/// The revision observed for a record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRevision {
    /// Canonical record identity.
    pub record_uid: String,
    /// Positive stored revision.
    pub revision: i64,
}

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}

fn validate_record_uid(record_uid: &str) -> Result<(), StoreError> {
    if !valid_uid(record_uid, "r") {
        return Err(protocol("Record revision requires a canonical Record uid"));
    }
    Ok(())
}

/// Reads and validates a record's revision on the caller's connection.
///
/// # Errors
/// Returns an error for an invalid identity, missing record or revision, or invalid stored revision.
pub async fn get_on(
    connection: &mut SqliteConnection,
    record_uid: &str,
) -> Result<RecordRevision, StoreError> {
    validate_record_uid(record_uid)?;
    let (present, kind, revision): (bool, String, Option<i64>) = query_as(
        "SELECT rr.record_uid IS NOT NULL,
                typeof(rr.revision),
                CASE WHEN typeof(rr.revision) = 'integer' THEN rr.revision END
           FROM record AS r
           LEFT JOIN record_revision AS rr ON rr.record_uid = r.uid
          WHERE r.uid = ?",
    )
    .bind(record_uid)
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| protocol("Record revision requires an existing Record"))?;
    if !present {
        return Err(protocol("Record is missing its revision"));
    }
    if kind != "integer" {
        return Err(protocol("Record revision has an invalid storage type"));
    }
    let revision = revision.ok_or_else(|| protocol("Record revision is unreadable"))?;
    if revision <= 0 {
        return Err(protocol("Record revision must be positive"));
    }
    Ok(RecordRevision {
        record_uid: record_uid.to_owned(),
        revision,
    })
}

/// Reads a revision and rejects it if it differs from the expected value.
///
/// # Errors
/// Returns an error for a nonpositive expected revision, mismatched revision, or failed read.
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

/// Checks that all observed revisions still match live records, in bounded batches.
///
/// # Errors
/// Returns an error if a batch cannot be fetched or decoded.
pub async fn live_match(pool: &SqlitePool, expected: &[(String, i64)]) -> Result<bool, StoreError> {
    let _read = Span::new("record-revision-observation-await");
    for expected in expected.chunks(BATCH_SIZE) {
        let mut query = QueryBuilder::<Sqlite>::new("WITH requested(position, uid) AS (");
        query.push_values(
            expected.iter().enumerate(),
            |mut tuple, (position, (uid, _))| {
                tuple.push(position).push_bind(uid);
            },
        );
        query.push(
            ") SELECT rr.revision, rr.record_uid IS NOT NULL AS present,
                      r.deleted_at IS NOT NULL AS deleted
               FROM requested
               LEFT JOIN record r ON r.uid = requested.uid
               LEFT JOIN record_revision rr ON rr.record_uid = r.uid
              ORDER BY requested.position",
        );
        let rows = query.build().fetch_all(pool).await?;
        add("record-revision-observation-batches", 1);
        add(
            "record-revision-observation-rows",
            u64::try_from(rows.len()).unwrap_or(u64::MAX),
        );
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

#[cfg(test)]
mod tests {
    use nucleus::RecordKind;

    use super::{BATCH_SIZE, get_on, live_match};
    use crate::{
        Store,
        exact::zero,
        records::{NewRecord, create, mark_deleted},
    };

    #[tokio::test]
    async fn live_revisions_compare_duplicate_rows_across_batches_and_reject_deleted_records() {
        let store = Store::open_memory().await.unwrap();
        let pool = &store.pool;
        let record = create(
            pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Observed",
                body: "",
                quantity: zero(),
            },
        )
        .await
        .unwrap();
        let mut connection = pool.acquire().await.unwrap();
        let revision = get_on(&mut connection, &record.uid).await.unwrap().revision;
        drop(connection);

        let mut expected = vec![(record.uid.clone(), revision); BATCH_SIZE + 1];
        assert!(live_match(pool, &[]).await.unwrap());
        assert!(live_match(pool, &expected).await.unwrap());
        expected.last_mut().unwrap().1 += 1;
        assert!(!live_match(pool, &expected).await.unwrap());

        assert!(mark_deleted(pool, &record.uid).await.unwrap());
        let mut connection = pool.acquire().await.unwrap();
        let revision = get_on(&mut connection, &record.uid).await.unwrap().revision;
        drop(connection);
        assert!(!live_match(pool, &[(record.uid, revision)]).await.unwrap());
    }
}
