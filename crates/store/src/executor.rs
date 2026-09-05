use sqlx::SqlitePool;

use crate::StoreError;

pub const NAMESPACE: &str = "lince.schedule.executor";

pub async fn designate(
    pool: &SqlitePool,
    record_uid: &str,
    cell_uid: Option<&str>,
) -> Result<(), StoreError> {
    crate::records::set_extension(
        pool,
        record_uid,
        NAMESPACE,
        &serde_json::json!({ "cell": cell_uid }),
    )
    .await
}

pub async fn designated(pool: &SqlitePool, record_uid: &str) -> Result<Option<String>, StoreError> {
    Ok(crate::records::get_extension(pool, record_uid, NAMESPACE)
        .await?
        .and_then(|fds| {
            fds.get("cell")
                .and_then(|cell| cell.as_str().map(str::to_string))
        }))
}

pub async fn runs_here(pool: &SqlitePool, record_uid: &str) -> Result<bool, StoreError> {
    let Some(designated_cell) = designated(pool, record_uid).await? else {
        return Ok(true);
    };
    Ok(crate::cells::local(pool)
        .await?
        .is_some_and(|cell| cell.uid == designated_cell))
}
