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
                actor_label
            ) VALUES
                (1, 1, 'transfer_created', '{}', 'me'),
                (2, 99, 'transfer_created', '{}', 'me');
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

        let party_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM transfer_party")
            .fetch_one(&pool)
            .await
            .expect("count transfer parties");
        assert_eq!(party_count, 2);

        let orphan_event_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(1) FROM transfer_event WHERE transfer_id = 99",
        )
        .fetch_one(&pool)
        .await
        .expect("count orphan transfer events");
        assert_eq!(orphan_event_count, 0);
    }
}
