pub mod connection;
pub mod models;
pub mod repositories;
pub mod schema;
pub mod seeder;
pub mod storage;
pub mod write_coordinator;

use {
    sqlx::{Pool, Sqlite},
    std::io::Error,
};

pub async fn bootstrap_database(db: &Pool<Sqlite>, local_base_url: &str) -> Result<(), Error> {
    sqlx::migrate!("../../migrations")
        .run(db)
        .await
        .map_err(Error::other)?;
    seeder::seed(db, local_base_url).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    #[tokio::test]
    async fn embedded_migrations_create_structured_transfer_tables() {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect to in-memory sqlite");

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("run embedded migrations");

        for table in [
            "transfer_party",
            "transfer_structured_item",
            "transfer_interaction",
            "transfer_agreement",
            "transfer_confirmation",
            "transfer_structured_settlement",
            "transfer_quantity_influence",
            "transfer_message",
            "transfer_visibility_subject",
            "transfer_visibility_rule",
            "transfer_visibility_field",
        ] {
            let exists = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("query sqlite_master");
            assert_eq!(exists, 1, "missing table {table}");
        }
    }
}
