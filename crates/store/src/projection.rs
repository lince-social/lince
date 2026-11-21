// non-lean-yet
// Scope: struct ManualInputs, fn manual_inputs, fn install, fn revision.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 51.
// - [Clippy: clippy::needless_pass_by_value] Borrow eligible inputs after checking caller and
//   framework contracts. Sites 445.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 269, 276, 281, 282, 340.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 12, 77, 160, 166, 196, 213, 221, 231, 315, 329, 346, 368.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 6, 178.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 7,
//   8, 9, 12, 77, 160, 166, 178, 179, 180, 181, 182, 196, 213, 221, 231, 315, 329, 346, 368.

use nucleus::projection::{Context, Incomplete, MAX_SPANS, Scheduled, Span, Status};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub struct ManualInputs {
    pub revision: i64,
    pub records: Vec<crate::records::RecordRow<Option<String>>>,
    pub work: std::collections::HashMap<String, serde_json::Value>,
}

pub async fn manual_inputs(pool: &SqlitePool) -> Result<Option<ManualInputs>, StoreError> {
    let mut transaction = pool.begin().await?;
    let revision = sqlx::query_scalar("SELECT revision FROM projection_source WHERE id = 1")
        .fetch_one(&mut *transaction)
        .await?;
    let simulation: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM recurrence)
             OR EXISTS(SELECT 1 FROM karma_program_revision)
             OR EXISTS(SELECT 1 FROM karma_frequency)
             OR EXISTS(SELECT 1 FROM karma_schedule)
             OR EXISTS(SELECT 1 FROM karma_schedule_cursor)
             OR EXISTS(SELECT 1 FROM karma_schedule_occurrence)
             OR EXISTS(SELECT 1 FROM karma_occurrence)
             OR EXISTS(SELECT 1 FROM karma_rule_application)
             OR EXISTS(SELECT 1 FROM signal)
             OR EXISTS(SELECT 1 FROM effect_queue WHERE status IN ('queued', 'running'))
             OR EXISTS(SELECT 1 FROM transfer_delivery_outbox WHERE status IN ('queued', 'failed'))
             OR EXISTS(SELECT 1 FROM promise)
             OR EXISTS(SELECT 1 FROM transfer_remote_reference)
             OR EXISTS(SELECT 1 FROM organ_roster)
             OR EXISTS(SELECT 1 FROM identity_key WHERE key_id = 'projection')
             OR NOT EXISTS(SELECT 1 FROM record WHERE slug = 'local-cell' AND kind = 'device')
             OR NOT EXISTS(SELECT 1 FROM record WHERE slug = 'local-organ' AND kind = 'organ')",
    )
    .fetch_one(&mut *transaction)
    .await?;
    if simulation {
        transaction.rollback().await?;
        return Ok(None);
    }
    let runtime: Option<String> = sqlx::query_scalar(
        "SELECT e.fds FROM record_extension e JOIN record r ON r.uid = e.record_uid
         WHERE r.slug = 'local-cell' AND r.kind = 'device' AND e.namespace = 'lince.karma-runtime'",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let running = runtime
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| value["running"].as_bool())
        .unwrap_or(true);
    if !running {
        transaction.rollback().await?;
        return Ok(None);
    }
    let records = crate::records::read_rows(
        &mut *transaction,
        crate::records::RecordSelection::All,
        false,
    )
    .await?;
    let work = crate::records::extension_rows(&mut *transaction, "work").await?;
    transaction.rollback().await?;
    let records = records
        .into_iter()
        .map(crate::records::map_row)
        .collect::<Result<_, _>>()?;
    let work = crate::records::map_extensions(work);
    Ok(Some(ManualInputs {
        revision,
        records,
        work,
    }))
}

pub async fn install(pool: &SqlitePool) -> Result<(), StoreError> {
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_list WHERE schema = 'main' AND type = 'table' AND substr(name, 1, 7) <> 'sqlite_' AND substr(name, 1, 5) <> '_sqlx' AND substr(name, 1, 11) <> 'projection_' ORDER BY name").fetch_all(pool).await?;
    let mut tx = crate::write_tx(pool).await?;
    for operation in ["INSERT", "UPDATE", "DELETE"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER IF EXISTS projection_{operation}_commit_sequence"
        )))
        .execute(&mut *tx)
        .await?;
    }
    for table in tables {
        if table.starts_with("interface_")
            || table.starts_with("plugin_")
            || table.starts_with("sync_activity")
            || table == "karma_deadline_lease"
            || table == "commit_sequence"
        {
            continue;
        }
        let table = table.replace('"', "\"\"");
        for operation in ["INSERT", "UPDATE", "DELETE"] {
            let mut conditions = Vec::new();
            if operation == "UPDATE" {
                let columns: Vec<String> =
                    sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
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
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "DROP TRIGGER IF EXISTS \"projection_{operation}_{table}\""
                )))
                .execute(&mut *tx)
                .await?;
            }
            let condition = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHEN {}", conditions.join(" AND "))
            };
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE TRIGGER IF NOT EXISTS \"projection_{operation}_{table}\" AFTER {operation} ON \"{table}\"{condition} BEGIN UPDATE projection_source SET revision = revision + 1 WHERE id = 1; END"))).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}

pub async fn revision(pool: &SqlitePool) -> Result<i64, StoreError> {
    sqlx::query_scalar("SELECT revision FROM projection_source WHERE id = 1")
        .fetch_one(pool)
        .await
}

pub async fn set_runtime(pool: &SqlitePool, runtime: &str) -> Result<(), StoreError> {
    let current: Option<String> =
        sqlx::query_scalar("SELECT runtime FROM projection_source WHERE id = 1")
            .fetch_one(pool)
            .await?;
    if current.as_deref() == Some(runtime) {
        return Ok(());
    }
    sqlx::query("UPDATE projection_source SET runtime = ?, revision = revision + 1 WHERE id = 1 AND runtime IS NOT ?").bind(runtime).bind(runtime).execute(pool).await?;
    Ok(())
}

pub struct Cached {
    pub status: Status,
    pub spans: Vec<Span>,
    pub schedule: Vec<Scheduled>,
    pub expires_ms: i64,
}

async fn current_window(
    connection: &mut sqlx::SqliteConnection,
    key: &str,
    context: &Context,
    now_ms: i64,
) -> Result<Option<sqlx::sqlite::SqliteRow>, StoreError> {
    sqlx::query("SELECT base_ms, expires_ms, incomplete FROM projection_window WHERE id = 1 AND cache_key = ? AND source_revision = (SELECT revision FROM projection_source WHERE id = 1) AND expires_ms > ? AND base_ms <= ? AND from_ms <= MAX(?, base_ms) AND until_ms >= ?")
        .bind(key).bind(now_ms).bind(now_ms).bind(context.window.from_ms).bind(context.window.until_ms)
        .fetch_optional(connection).await
}

pub async fn cached_until(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<i64>, StoreError> {
    let Some(identity) = store.execution_identity() else {
        return Ok(None);
    };
    let key = context.key(identity).map_err(protocol)?;
    let mut connection = store.pool.acquire().await?;
    Ok(
        current_window(&mut connection, key.as_str(), context, now_ms)
            .await?
            .map(|row| row.get("expires_ms")),
    )
}

pub async fn read(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<Cached>, StoreError> {
    read_content(store, context, now_ms, Content::All).await
}

pub async fn read_calendar(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(Status, Vec<Span>)>, StoreError> {
    Ok(read_content(store, context, now_ms, Content::Calendar)
        .await?
        .map(|cached| (cached.status, cached.spans)))
}

pub async fn read_schedule(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(Status, Vec<Scheduled>)>, StoreError> {
    Ok(read_content(store, context, now_ms, Content::Schedule)
        .await?
        .map(|cached| (cached.status, cached.schedule)))
}

#[derive(Clone, Copy)]
enum Content {
    All,
    Calendar,
    Schedule,
}

async fn read_content(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
    content: Content,
) -> Result<Option<Cached>, StoreError> {
    let _read = nucleus::performance::Span::new("projection-cache-read-await");
    let Some(identity) = store.execution_identity() else {
        return Ok(None);
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    let mut tx = pool.begin().await?;
    let row = current_window(&mut tx, key.as_str(), context, now_ms).await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };
    let span_rows = if matches!(content, Content::All | Content::Calendar) {
        nucleus::performance::add("projection-span-reads", 1);
        sqlx::query("SELECT payload FROM projection_span WHERE from_ms < ? AND until_ms > ? ORDER BY from_ms, id LIMIT ?")
            .bind(context.window.until_ms).bind(context.window.from_ms).bind(MAX_SPANS as i64).fetch_all(&mut *tx).await?
    } else {
        Vec::new()
    };
    let schedule_rows = if matches!(content, Content::All | Content::Schedule) {
        nucleus::performance::add("projection-schedule-reads", 1);
        sqlx::query("SELECT payload FROM projection_schedule WHERE from_ms < ? AND ((until_ms IS NULL AND from_ms >= ?) OR until_ms > ?) ORDER BY from_ms, id LIMIT ?")
            .bind(context.window.until_ms).bind(context.window.from_ms).bind(context.window.from_ms).bind(MAX_SPANS as i64).fetch_all(&mut *tx).await?
    } else {
        Vec::new()
    };
    tx.rollback().await?;
    nucleus::performance::add("projection-span-rows", span_rows.len() as u64);
    nucleus::performance::add("projection-schedule-rows", schedule_rows.len() as u64);
    let incomplete: Option<&str> = row.get("incomplete");
    let status = match incomplete {
        Some(incomplete) => Status::Incomplete {
            base_ms: row.get("base_ms"),
            reason: serde_json::from_str(incomplete).map_err(protocol)?,
        },
        None => Status::Ready {
            base_ms: row.get("base_ms"),
            expires_ms: row.get("expires_ms"),
        },
    };
    let spans = span_rows
        .iter()
        .map(|row| {
            let mut span: Span = serde_json::from_str(row.get("payload")).map_err(protocol)?;
            span.from_ms = span.from_ms.max(context.window.from_ms);
            span.until_ms = span.until_ms.min(context.window.until_ms);
            Ok::<Span, StoreError>(span)
        })
        .collect::<Result<Vec<Span>, _>>()?;
    let schedule = schedule_rows
        .iter()
        .map(|row| serde_json::from_str(row.get("payload")).map_err(protocol))
        .collect::<Result<Vec<Scheduled>, StoreError>>()?;
    Ok(Some(Cached {
        status,
        spans,
        schedule,
        expires_ms: row.get("expires_ms"),
    }))
}

pub async fn covered_window(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(i64, i64)>, StoreError> {
    let Some(identity) = store.execution_identity() else {
        return Ok(None);
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    sqlx::query_as("SELECT from_ms, until_ms FROM projection_window WHERE id = 1 AND cache_key = ? AND source_revision = (SELECT revision FROM projection_source WHERE id = 1) AND expires_ms > ? AND base_ms <= ?")
        .bind(key.as_str()).bind(now_ms).bind(now_ms).fetch_optional(pool).await
}

pub async fn previous_schedule(
    store: &crate::Store,
    context: &Context,
    now_ms: i64,
) -> Result<Vec<Scheduled>, StoreError> {
    let Some(identity) = store.execution_identity() else {
        return Ok(Vec::new());
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    let rows = sqlx::query("SELECT s.payload FROM projection_schedule s JOIN projection_window w ON w.id = 1 WHERE w.cache_key = ? AND w.base_ms <= ? AND w.expires_ms > ? AND s.from_ms >= ? AND s.from_ms < ? ORDER BY s.from_ms, s.id LIMIT ?")
        .bind(key.as_str()).bind(now_ms).bind(now_ms.saturating_sub(60_000)).bind(now_ms.max(context.window.from_ms)).bind(context.window.until_ms).bind(MAX_SPANS as i64).fetch_all(pool).await?;
    rows.iter()
        .map(|row| serde_json::from_str(row.get("payload")).map_err(protocol))
        .collect()
}

pub async fn publish(
    store: &crate::Store,
    context: &Context,
    source: i64,
    base_ms: i64,
    expires_ms: i64,
    incomplete: Option<&Incomplete>,
    spans: &[Span],
) -> Result<bool, StoreError> {
    publish_schedule(
        store,
        context,
        source,
        base_ms,
        expires_ms,
        incomplete,
        spans,
        &[],
    )
    .await
}

pub async fn publish_schedule(
    store: &crate::Store,
    context: &Context,
    source: i64,
    base_ms: i64,
    expires_ms: i64,
    incomplete: Option<&Incomplete>,
    spans: &[Span],
    schedule: &[Scheduled],
) -> Result<bool, StoreError> {
    if spans.len() > MAX_SPANS || schedule.len() > MAX_SPANS {
        return Err(protocol("projection span budget exceeded"));
    }
    let encoded = spans
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(protocol)?;
    let encoded_schedule = schedule
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(protocol)?;
    if encoded
        .iter()
        .chain(&encoded_schedule)
        .map(String::len)
        .sum::<usize>()
        > nucleus::projection::MAX_CACHE_BYTES
    {
        return Err(protocol("projection byte budget exceeded"));
    }
    let Some(identity) = store.execution_identity() else {
        return Err(protocol(
            "projection publication requires execution identity",
        ));
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    let mut tx = crate::write_tx(pool).await?;
    let current: i64 = sqlx::query_scalar("SELECT revision FROM projection_source WHERE id = 1")
        .fetch_one(&mut *tx)
        .await?;
    if current != source {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query("DELETE FROM projection_span")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM projection_schedule")
        .execute(&mut *tx)
        .await?;
    for (span, payload) in spans.iter().zip(encoded) {
        sqlx::query("INSERT INTO projection_span VALUES (?, ?, ?, ?, ?)")
            .bind(&span.id)
            .bind(span.record.as_str())
            .bind(span.from_ms)
            .bind(span.until_ms)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
    }
    for (entry, payload) in schedule.iter().zip(encoded_schedule) {
        sqlx::query("INSERT INTO projection_schedule VALUES (?, ?, ?, ?, ?)")
            .bind(&entry.id)
            .bind(entry.record.as_str())
            .bind(entry.time.from_ms)
            .bind(entry.time.until_ms)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("INSERT INTO projection_window VALUES (1, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET cache_key = excluded.cache_key, source_revision = excluded.source_revision, base_ms = excluded.base_ms, expires_ms = excluded.expires_ms, from_ms = excluded.from_ms, until_ms = excluded.until_ms, incomplete = excluded.incomplete")
        .bind(key.as_str()).bind(source).bind(base_ms).bind(expires_ms).bind(context.window.from_ms).bind(context.window.until_ms).bind(incomplete.map(serde_json::to_string).transpose().map_err(protocol)?).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn protocol(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delivery_bookkeeping_preserves_forecasts_unless_programs_can_read_it() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql("CREATE TABLE commit_sequence (id INTEGER PRIMARY KEY, value INTEGER); INSERT INTO commit_sequence VALUES (1, 0); CREATE TABLE projection_source (id INTEGER PRIMARY KEY, revision INTEGER); INSERT INTO projection_source VALUES (1, 0); CREATE TABLE peer_delivery (attempted_at TEXT, error TEXT); CREATE TABLE record (head TEXT); CREATE TABLE karma_program_revision (revision_hash TEXT); CREATE TRIGGER projection_UPDATE_peer_delivery AFTER UPDATE ON peer_delivery BEGIN UPDATE projection_source SET revision = revision + 1; END;")
            .execute(&pool).await.unwrap();
        install(&pool).await.unwrap();
        sqlx::query("INSERT INTO peer_delivery VALUES ('first', 'offline')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE peer_delivery SET attempted_at = 'retry', error = 'still offline'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), 0);
        sqlx::query("INSERT INTO record VALUES ('Task')")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), 1);
        sqlx::query("INSERT INTO karma_program_revision VALUES ('program')")
            .execute(&pool)
            .await
            .unwrap();
        let before = revision(&pool).await.unwrap();
        sqlx::query("UPDATE peer_delivery SET attempted_at = 'program-visible retry'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), before + 1);
        sqlx::query("DELETE FROM peer_delivery")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(revision(&pool).await.unwrap(), before + 2);
    }
}
