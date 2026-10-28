use crate::{
    StoreError,
    sqlx::{Sqlite, Transaction},
};

pub async fn remember_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    source: &str,
    document: &str,
    hash: &str,
) -> Result<(), StoreError> {
    if !nucleus::valid_uid(record, "r")
        || !nucleus::valid_uid(source, "r")
        || document.len() > 8100
        || document.contains('#')
        || !document
            .split_once(':')
            .is_some_and(|(scheme, _)| matches!(scheme, "file" | "http" | "https"))
    {
        return Err(StoreError::Protocol(
            "Invalid accepted Record source address".into(),
        ));
    }
    let uri = format!("{document}#{source}");
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM record_source_location WHERE record_uid=? AND uri<>?",
    )
    .bind(record)
    .bind(&uri)
    .fetch_one(&mut **tx)
    .await?;
    if count >= 64 {
        return Err(StoreError::Protocol(
            "A Record has more than 64 accepted source locations".into(),
        ));
    }
    sqlx::query("INSERT INTO record_source_location(record_uid,source_uid,uri,hash) VALUES(?,?,?,?) ON CONFLICT(record_uid,uri) DO UPDATE SET hash=excluded.hash")
        .bind(record).bind(source).bind(uri).bind(hash).execute(&mut **tx).await?;
    Ok(())
}
