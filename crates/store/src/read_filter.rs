use sqlx::SqlitePool;

use crate::StoreError;

pub async fn get(pool: &SqlitePool, person_uid: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT read_filter FROM person_credential WHERE person_uid = ?")
            .bind(person_uid)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

pub async fn set(
    pool: &SqlitePool,
    person_uid: &str,
    predicate: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE person_credential SET read_filter = ? WHERE person_uid = ?")
        .bind(predicate)
        .bind(person_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn everyone(pool: &SqlitePool) -> Result<Vec<(String, Option<String>)>, StoreError> {
    use sqlx::Row;
    Ok(
        sqlx::query("SELECT person_uid, read_filter FROM person_credential ORDER BY person_uid")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.get("person_uid"), row.get("read_filter")))
            .collect(),
    )
}
