use sqlx::{SqliteConnection, SqlitePool};

use crate::StoreError;

pub async fn get(pool: &SqlitePool, person_uid: &str) -> Result<Option<String>, StoreError> {
    Ok(crate::auth::person_access(pool, person_uid)
        .await?
        .and_then(|access| access.read_filter))
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(crate::auth::person_access_on(connection, person_uid)
        .await?
        .and_then(|access| access.read_filter))
}

pub async fn set(
    pool: &SqlitePool,
    person_uid: &str,
    predicate: Option<&str>,
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let expected = crate::auth::person_access_on(&mut tx, person_uid)
        .await?
        .map_or(0, |access| access.revision);
    crate::auth::compare_and_set_read_filter_on(&mut tx, person_uid, predicate, expected).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn compare_and_set(
    pool: &SqlitePool,
    person_uid: &str,
    predicate: Option<&str>,
    expected_revision: i64,
) -> Result<crate::auth::PersonAccess, StoreError> {
    crate::auth::compare_and_set_read_filter(pool, person_uid, predicate, expected_revision).await
}

pub async fn compare_and_set_on(
    connection: &mut SqliteConnection,
    person_uid: &str,
    predicate: Option<&str>,
    expected_revision: i64,
) -> Result<crate::auth::PersonAccess, StoreError> {
    crate::auth::compare_and_set_read_filter_on(
        connection,
        person_uid,
        predicate,
        expected_revision,
    )
    .await
}

pub async fn everyone(pool: &SqlitePool) -> Result<Vec<(String, Option<String>)>, StoreError> {
    use sqlx::Row;
    Ok(sqlx::query(
        "SELECT a.person_uid, a.read_filter
               FROM person_access a
               JOIN record p ON p.uid = a.person_uid
                    AND p.kind = 'person' AND p.deleted_at IS NULL
              ORDER BY a.person_uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("person_uid"), row.get("read_filter")))
    .collect())
}
