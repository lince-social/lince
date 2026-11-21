use crate::{StoreError, write_tx};
use sqlx::{AssertSqlSafe, SqlitePool, query, query_scalar};

const OPERATIONS: [&str; 3] = ["INSERT", "UPDATE", "DELETE"];

/// Installs or refreshes triggers that invalidate forecasts when source data changes.
///
/// # Errors
///
/// Returns an error if SQLite cannot read the schema or update the triggers.
pub async fn install(pool: &SqlitePool) -> Result<(), StoreError> {
    let tables: Vec<String> = query_scalar("SELECT name FROM pragma_table_list WHERE schema = 'main' AND type = 'table' AND substr(name, 1, 7) <> 'sqlite_' AND substr(name, 1, 5) <> '_sqlx' AND substr(name, 1, 11) <> 'projection_' ORDER BY name").fetch_all(pool).await?;
    let mut tx = write_tx(pool).await?;
    for operation in OPERATIONS {
        query(AssertSqlSafe(format!(
            "DROP TRIGGER IF EXISTS projection_{operation}_commit_sequence"
        )))
        .execute(&mut *tx)
        .await?;
    }
    for table in tables {
        if ["interface_", "plugin_", "sync_activity"]
            .into_iter()
            .any(|prefix| table.starts_with(prefix))
            || matches!(table.as_str(), "karma_deadline_lease" | "commit_sequence")
        {
            continue;
        }
        let identifier = table.replace('"', "\"\"");
        for operation in OPERATIONS {
            let mut conditions = Vec::new();
            if operation == "UPDATE" {
                let columns: Vec<String> = query_scalar("SELECT name FROM pragma_table_info(?)")
                    .bind(&table)
                    .fetch_all(&mut *tx)
                    .await?;
                let changed: Vec<_> = columns
                    .iter()
                    .map(|name| {
                        let name = name.replace('"', "\"\"");
                        format!("old.\"{name}\" IS NOT new.\"{name}\"")
                    })
                    .collect();
                conditions.push(format!("({})", changed.join(" OR ")));
                if table == "karma_schedule_cursor" {
                    let changes: Vec<_> = columns
                        .iter()
                        .zip(&changed)
                        .filter(|(name, _)| !matches!(name.as_str(), "admitted_at" | "updated_at"))
                        .map(|(_, change)| change.as_str())
                        .collect();
                    conditions.push(format!(
                        "(EXISTS (SELECT 1 FROM karma_program_revision) OR ({}))",
                        changes.join(" OR ")
                    ));
                }
            }
            if matches!(table.as_str(), "record_revision" | "peer_delivery") {
                conditions.push("EXISTS (SELECT 1 FROM karma_program_revision)".into());
            }
            if table == "record_extension" {
                let namespace = match operation {
                    "INSERT" => "new.namespace <> 'lince.pairing'",
                    "DELETE" => "old.namespace <> 'lince.pairing'",
                    _ => "old.namespace <> 'lince.pairing' OR new.namespace <> 'lince.pairing'",
                };
                conditions.push(format!(
                    "({namespace} OR EXISTS (SELECT 1 FROM karma_program_revision))"
                ));
            }
            if matches!(
                table.as_str(),
                "record_revision" | "record_extension" | "karma_schedule_cursor" | "peer_delivery"
            ) {
                query(AssertSqlSafe(format!(
                    "DROP TRIGGER IF EXISTS \"projection_{operation}_{identifier}\""
                )))
                .execute(&mut *tx)
                .await?;
            }
            let condition = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHEN {}", conditions.join(" AND "))
            };
            query(AssertSqlSafe(format!("CREATE TRIGGER IF NOT EXISTS \"projection_{operation}_{identifier}\" AFTER {operation} ON \"{identifier}\"{condition} BEGIN UPDATE projection_source SET revision = revision + 1 WHERE id = 1; END"))).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::install;
    use crate::projection::revision;
    use sqlx::{query, raw_sql, sqlite::SqlitePoolOptions};

    #[tokio::test]
    async fn delivery_bookkeeping_preserves_forecasts_unless_programs_can_read_it() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        raw_sql("CREATE TABLE commit_sequence (id INTEGER PRIMARY KEY, value INTEGER); INSERT INTO commit_sequence VALUES (1, 0); CREATE TABLE projection_source (id INTEGER PRIMARY KEY, revision INTEGER); INSERT INTO projection_source VALUES (1, 0); CREATE TABLE peer_delivery (attempted_at TEXT, error TEXT); CREATE TABLE record (head TEXT); CREATE TABLE karma_program_revision (revision_hash TEXT); CREATE TRIGGER projection_UPDATE_peer_delivery AFTER UPDATE ON peer_delivery BEGIN UPDATE projection_source SET revision = revision + 1; END;")
            .execute(&pool).await.unwrap();
        install(&pool).await.unwrap();
        query("INSERT INTO peer_delivery VALUES ('first', 'offline')")
            .execute(&pool)
            .await
            .unwrap();
        query("UPDATE peer_delivery SET attempted_at = 'retry', error = 'still offline'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), 0);
        query("INSERT INTO record VALUES ('Task')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), 1);
        query("INSERT INTO karma_program_revision VALUES ('program')")
            .execute(&pool)
            .await
            .unwrap();
        let before = revision(&pool).await.unwrap();
        query("UPDATE peer_delivery SET attempted_at = 'program-visible retry'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), before + 1);
        query("DELETE FROM peer_delivery")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), before + 2);
    }

    #[tokio::test]
    async fn quoted_table_names_register_projection_triggers() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        raw_sql(
            r#"
            CREATE TABLE commit_sequence (id INTEGER PRIMARY KEY, value INTEGER);
            INSERT INTO commit_sequence VALUES (1, 0);
            CREATE TABLE projection_source (id INTEGER PRIMARY KEY, revision INTEGER);
            INSERT INTO projection_source VALUES (1, 0);
            CREATE TABLE "quoted""table" (value TEXT);
        "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        install(&pool).await.unwrap();
        query(r#"INSERT INTO "quoted""table" VALUES ('old')"#)
            .execute(&pool)
            .await
            .unwrap();
        query(r#"UPDATE "quoted""table" SET value = 'new'"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), 2);
    }
}
