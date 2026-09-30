use nucleus::projection::{Context, Incomplete, MAX_SPANS, Span, Status};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub async fn install(pool: &SqlitePool) -> Result<(), StoreError> {
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_list WHERE schema = 'main' AND type = 'table' AND substr(name, 1, 7) <> 'sqlite_' AND substr(name, 1, 5) <> '_sqlx' AND substr(name, 1, 11) <> 'projection_' ORDER BY name").fetch_all(pool).await?;
    let mut tx = crate::write_tx(pool).await?;
    for table in tables {
        if table.starts_with("interface_")
            || table.starts_with("sync_activity")
            || table == "karma_deadline_lease"
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
            if table == "record_revision" {
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
                "record_revision" | "record_extension" | "karma_schedule_cursor"
            ) {
                sqlx::query(&format!(
                    "DROP TRIGGER IF EXISTS \"projection_{operation}_{table}\""
                ))
                .execute(&mut *tx)
                .await?;
            }
            let condition = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHEN {}", conditions.join(" AND "))
            };
            sqlx::query(&format!("CREATE TRIGGER IF NOT EXISTS \"projection_{operation}_{table}\" AFTER {operation} ON \"{table}\"{condition} BEGIN UPDATE projection_source SET revision = revision + 1 WHERE id = 1; END")).execute(&mut *tx).await?;
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
    sqlx::query("UPDATE projection_source SET runtime = ?, revision = revision + 1 WHERE id = 1 AND runtime IS NOT ?").bind(runtime).bind(runtime).execute(pool).await?;
    Ok(())
}

pub struct Cached {
    pub status: Status,
    pub spans: Vec<Span>,
    pub expires_ms: i64,
}

pub async fn read(
    pool: &SqlitePool,
    context: &Context,
    now_ms: i64,
) -> Result<Option<Cached>, StoreError> {
    let key = context.key().map_err(protocol)?;
    let mut tx = pool.begin().await?;
    let row = sqlx::query("SELECT base_ms, expires_ms, incomplete FROM projection_window WHERE id = 1 AND cache_key = ? AND source_revision = (SELECT revision FROM projection_source WHERE id = 1) AND expires_ms > ? AND base_ms <= ? AND from_ms <= MAX(?, base_ms) AND until_ms >= ?")
        .bind(key.as_str()).bind(now_ms).bind(now_ms).bind(context.window.from_ms).bind(context.window.until_ms).fetch_optional(&mut *tx).await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };
    let incomplete: Option<String> = row.get("incomplete");
    let status = match incomplete {
        Some(incomplete) => Status::Incomplete {
            base_ms: row.get("base_ms"),
            reason: serde_json::from_str(&incomplete).map_err(protocol)?,
        },
        None => Status::Ready {
            base_ms: row.get("base_ms"),
            expires_ms: row.get("expires_ms"),
        },
    };
    let rows: Vec<String> = sqlx::query_scalar("SELECT payload FROM projection_span WHERE from_ms < ? AND until_ms > ? ORDER BY from_ms, id LIMIT ?")
        .bind(context.window.until_ms).bind(context.window.from_ms).bind(MAX_SPANS as i64).fetch_all(&mut *tx).await?;
    let spans = rows
        .iter()
        .map(|row| {
            let mut span: Span = serde_json::from_str(row).map_err(protocol)?;
            span.from_ms = span.from_ms.max(context.window.from_ms);
            span.until_ms = span.until_ms.min(context.window.until_ms);
            Ok::<Span, StoreError>(span)
        })
        .collect::<Result<Vec<Span>, _>>()?;
    tx.rollback().await?;
    Ok(Some(Cached {
        status,
        spans,
        expires_ms: row.get("expires_ms"),
    }))
}

pub async fn covered_window(
    pool: &SqlitePool,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(i64, i64)>, StoreError> {
    let key = context.key().map_err(protocol)?;
    sqlx::query_as("SELECT from_ms, until_ms FROM projection_window WHERE id = 1 AND cache_key = ? AND source_revision = (SELECT revision FROM projection_source WHERE id = 1) AND expires_ms > ? AND base_ms <= ?")
        .bind(key.as_str()).bind(now_ms).bind(now_ms).fetch_optional(pool).await
}

pub async fn publish(
    pool: &SqlitePool,
    context: &Context,
    source: i64,
    base_ms: i64,
    expires_ms: i64,
    incomplete: Option<&Incomplete>,
    spans: &[Span],
) -> Result<bool, StoreError> {
    if spans.len() > MAX_SPANS {
        return Err(protocol("projection span budget exceeded"));
    }
    let encoded = spans
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(protocol)?;
    if encoded.iter().map(String::len).sum::<usize>() > 16 * 1024 * 1024 {
        return Err(protocol("projection byte budget exceeded"));
    }
    let key = context.key().map_err(protocol)?;
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
    sqlx::query("INSERT INTO projection_window VALUES (1, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET cache_key = excluded.cache_key, source_revision = excluded.source_revision, base_ms = excluded.base_ms, expires_ms = excluded.expires_ms, from_ms = excluded.from_ms, until_ms = excluded.until_ms, incomplete = excluded.incomplete")
        .bind(key.as_str()).bind(source).bind(base_ms).bind(expires_ms).bind(context.window.from_ms).bind(context.window.until_ms).bind(incomplete.map(serde_json::to_string).transpose().map_err(protocol)?).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn protocol(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}
