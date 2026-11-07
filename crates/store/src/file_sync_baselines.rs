use crate::{
    StoreError,
    sqlx::{Sqlite, SqlitePool, Transaction},
};

pub async fn load(
    pool: &SqlitePool,
    organ: &str,
    directory: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    sqlx::query_as("SELECT path, document FROM file_sync_baseline WHERE organ_uid = ? AND directory = ? ORDER BY path")
        .bind(organ).bind(directory).fetch_all(pool).await
}

pub async fn save_on(
    tx: &mut Transaction<'_, Sqlite>,
    organ: &str,
    directory: &str,
    path: &str,
    document: &str,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO file_sync_baseline(organ_uid, directory, path, document) VALUES (?, ?, ?, ?) ON CONFLICT(organ_uid, directory, path) DO UPDATE SET document = excluded.document WHERE document != excluded.document")
        .bind(organ).bind(directory).bind(path).bind(document).execute(&mut **tx).await?;
    Ok(())
}

pub async fn remove(
    pool: &SqlitePool,
    organ: &str,
    directory: &str,
    path: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM file_sync_baseline WHERE organ_uid = ? AND directory = ? AND path = ?",
    )
    .bind(organ)
    .bind(directory)
    .bind(path)
    .execute(pool)
    .await?;
    Ok(())
}
