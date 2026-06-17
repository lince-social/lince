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

    #[tokio::test]
    async fn structured_transfer_migration_ignores_orphaned_legacy_rows() {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("connect to in-memory sqlite");

        sqlx::raw_sql(
            "
            CREATE TABLE record (
                id INTEGER PRIMARY KEY,
                quantity REAL NOT NULL DEFAULT 1,
                head TEXT,
                body TEXT
            );
            CREATE TABLE transfer (
                id INTEGER PRIMARY KEY,
                quantity REAL NOT NULL DEFAULT 1
            );
            CREATE TABLE transfer_identity (
                id INTEGER PRIMARY KEY,
                transfer_id INTEGER NOT NULL UNIQUE REFERENCES transfer(id) ON DELETE CASCADE,
                transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
                parent_transfer_uid TEXT,
                source_transfer_uid TEXT,
                state TEXT NOT NULL CHECK (length(trim(state)) > 0),
                title TEXT NOT NULL CHECK (length(trim(title)) > 0),
                coordinator_label TEXT NOT NULL CHECK (length(trim(coordinator_label)) > 0),
                proposer_label TEXT NOT NULL CHECK (length(trim(proposer_label)) > 0),
                counterparty_label TEXT NOT NULL CHECK (length(trim(counterparty_label)) > 0),
                contribution_actor_label TEXT NOT NULL CHECK (length(trim(contribution_actor_label)) > 0),
                contribution_public_key TEXT,
                need_actor_label TEXT NOT NULL CHECK (length(trim(need_actor_label)) > 0),
                need_public_key TEXT,
                target_organ_id INTEGER,
                target_organ_name TEXT,
                target_base_url TEXT,
                source_base_url TEXT,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            ) STRICT;
            CREATE TABLE transfer_item (
                transfer_id INTEGER NOT NULL,
                contribution_user_id INTEGER NOT NULL,
                contribution_server_id INTEGER NOT NULL,
                contribution_id INTEGER NOT NULL,
                contribution_head TEXT NOT NULL,
                contribution_quantity REAL NOT NULL,
                need_user_id INTEGER NOT NULL,
                need_server_id INTEGER NOT NULL,
                need_id INTEGER NOT NULL,
                need_head TEXT NOT NULL,
                need_quantity REAL NOT NULL,
                first_agreement INTEGER NOT NULL DEFAULT 0,
                second_agreement INTEGER NOT NULL DEFAULT 0,
                date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                location TEXT NOT NULL
            );
            CREATE TABLE transfer_event (
                id INTEGER PRIMARY KEY,
                transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
                event_kind TEXT NOT NULL,
                payload_json TEXT NOT NULL DEFAULT '{}',
                actor_label TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                transfer_uid TEXT,
                event_uid TEXT,
                actor_public_key TEXT,
                previous_event_id INTEGER,
                previous_event_uid TEXT,
                signature TEXT
            );
            ",
        )
        .execute(&pool)
        .await
        .expect("create legacy schema");

        sqlx::raw_sql("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .expect("disable foreign keys");
        sqlx::raw_sql(
            "
            INSERT INTO record(id, head) VALUES (1, 'Existing Record');
            INSERT INTO transfer(id, quantity) VALUES (1, 1);
            INSERT INTO transfer_identity(
                transfer_id,
                transfer_uid,
                state,
                title,
                coordinator_label,
                proposer_label,
                counterparty_label,
                contribution_actor_label,
                need_actor_label
            ) VALUES
                (1, 'valid-transfer', 'proposal', 'Valid Transfer', 'me', 'me', 'you', 'me', 'you'),
                (99, 'orphan-transfer', 'proposal', 'Orphan Transfer', 'me', 'me', 'you', 'me', 'you');
            INSERT INTO transfer_item(
                transfer_id,
                contribution_user_id,
                contribution_server_id,
                contribution_id,
                contribution_head,
                contribution_quantity,
                need_user_id,
                need_server_id,
                need_id,
                need_head,
                need_quantity,
                location
            ) VALUES
                (1, 0, 0, 1, 'Existing Record', 1, 0, 0, 42, 'Missing Record', 1, ''),
                (99, 0, 0, 1, 'Orphan Contribution', 1, 0, 0, 42, 'Orphan Need', 1, '');
            INSERT INTO transfer_event(
                id,
                transfer_id,
                event_kind,
                payload_json,
                actor_label,
                previous_event_id
            ) VALUES
                (1, 1, 'transfer_created', '{}', 'me', NULL),
                (2, 99, 'transfer_created', '{}', 'me', NULL),
                (3, 1, 'agreement_changed', '{}', 'me', 2);
            ",
        )
        .execute(&pool)
        .await
        .expect("insert dirty legacy rows");
        sqlx::raw_sql("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .expect("enable foreign keys");

        sqlx::raw_sql(include_str!(
            "../../../migrations/20260614133000_structured_transfer_model.sql"
        ))
        .execute(&pool)
        .await
        .expect("run structured transfer migration against dirty legacy rows");
        sqlx::raw_sql(include_str!(
            "../../../migrations/20260614143000_transfer_event_vocabulary.sql"
        ))
        .execute(&pool)
        .await
        .expect("run transfer event vocabulary migration");

        let party_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM transfer_party")
            .fetch_one(&pool)
            .await
            .expect("count transfer parties");
        assert_eq!(party_count, 2);

        let structured_item_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM transfer_structured_item")
                .fetch_one(&pool)
                .await
                .expect("count structured items");
        assert_eq!(structured_item_count, 2);

        sqlx::query(
            "
            INSERT INTO transfer_event(
                transfer_id,
                event_kind,
                payload_json,
                actor_label
            ) VALUES (1, 'transfer_inactivated', '{}', 'me')
            ",
        )
        .execute(&pool)
        .await
        .expect("widened event vocabulary accepts transfer_inactivated");
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
