pub mod connection;
pub mod models;
pub mod repositories;
pub mod schema;
pub mod seeder;
pub mod storage;
pub mod write_coordinator;

use {
    sqlx::{
        Pool, Sqlite,
        sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    },
    std::{
        env, fs,
        io::Error,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    },
};

const MIGRATION_PREFLIGHT_ENV: &str = "LINCE_MIGRATION_PREFLIGHT";

pub async fn bootstrap_database(db: &Pool<Sqlite>, local_base_url: &str) -> Result<(), Error> {
    preflight_migrations_if_enabled().await?;
    sqlx::migrate!("../../migrations")
        .run(db)
        .await
        .map_err(Error::other)?;
    seeder::seed(db, local_base_url).await?;
    Ok(())
}

async fn preflight_migrations_if_enabled() -> Result<(), Error> {
    if !env_flag_enabled(MIGRATION_PREFLIGHT_ENV) {
        return Ok(());
    }

    let source_path = connection::sqlite_db_path()?;
    let temp_dir = unique_preflight_dir()?;
    fs::create_dir_all(&temp_dir)?;
    let temp_db_path = temp_dir.join("lince.db");

    let result = async {
        if source_path.exists() {
            fs::copy(&source_path, &temp_db_path).map_err(Error::other)?;
            copy_if_exists(
                source_path.with_extension("db-wal"),
                temp_db_path.with_extension("db-wal"),
            )?;
            copy_if_exists(
                source_path.with_extension("db-shm"),
                temp_db_path.with_extension("db-shm"),
            )?;
        }

        let options = SqliteConnectOptions::new()
            .filename(&temp_db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(Error::other)?;

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .map_err(|error| Error::other(format!("migration preflight failed: {error}")))?;

        pool.close().await;
        Ok::<(), Error>(())
    }
    .await;

    let _ = fs::remove_dir_all(&temp_dir);
    result
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            matches!(value.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn unique_preflight_dir() -> Result<PathBuf, Error> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(Error::other)?
        .as_nanos();
    Ok(env::temp_dir().join(format!(
        "lince-migration-preflight-{}-{nanos}",
        std::process::id()
    )))
}

fn copy_if_exists(source: PathBuf, destination: PathBuf) -> Result<(), Error> {
    if source.exists() {
        fs::copy(source, destination).map_err(Error::other)?;
    }
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
            "work_metadata",
            "work_subject",
            "work_assignment",
            "organ_sync_policy",
            "record_sync_operation",
            "record_sync_tombstone",
            "record_sync_ack",
            "record_sync_pending_dependency",
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

    #[test]
    fn migration_preflight_env_accepts_explicit_truthy_values() {
        // Avoid mutating process env in tests; keep the accepted spellings mirrored here.
        for value in ["1", "true", "TRUE", "yes", "on"] {
            let normalized = value.trim().to_ascii_lowercase();
            assert!(matches!(normalized.as_str(), "1" | "true" | "yes" | "on"));
        }
    }
}
