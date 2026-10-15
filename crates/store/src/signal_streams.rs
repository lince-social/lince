use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Configuration {
    pub signal_uid: String,
    pub value_type: String,
    pub source_url: String,
    pub revision: i64,
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    signal: &str,
) -> Result<Option<Configuration>, crate::StoreError> {
    sqlx::query_as("SELECT * FROM signal_stream WHERE signal_uid = ?")
        .bind(signal)
        .fetch_optional(connection)
        .await
}

pub async fn save_on(
    connection: &mut SqliteConnection,
    config: &Configuration,
) -> Result<(), crate::StoreError> {
    sqlx::query("INSERT INTO signal_stream (signal_uid, value_type, source_url, revision) VALUES (?, ?, ?, ?) ON CONFLICT(signal_uid) DO UPDATE SET value_type=excluded.value_type, source_url=excluded.source_url, revision=excluded.revision")
        .bind(&config.signal_uid).bind(&config.value_type).bind(&config.source_url).bind(config.revision)
        .execute(connection).await?;
    Ok(())
}

pub async fn remove_on(
    connection: &mut SqliteConnection,
    signal: &str,
) -> Result<(), crate::StoreError> {
    sqlx::query("DELETE FROM signal_stream WHERE signal_uid = ?")
        .bind(signal)
        .execute(connection)
        .await?;
    Ok(())
}
