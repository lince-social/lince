use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramExecutionRow {
    pub program_uid: String,
    pub slug: String,
    pub executes: bool,
    pub note: Option<String>,
    pub updated_at: Option<String>,
}

pub async fn executes(pool: &SqlitePool, program_uid: &str) -> Result<bool, StoreError> {
    let off: Option<i64> =
        sqlx::query_scalar("SELECT executes FROM karma_program_execution WHERE program_uid = ?")
            .bind(program_uid)
            .fetch_optional(pool)
            .await?;
    Ok(off != Some(0))
}

pub async fn set_executes(
    pool: &SqlitePool,
    program_uid: &str,
    executes: bool,
    note: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let known: Option<String> =
        sqlx::query_scalar("SELECT record_uid FROM karma_program WHERE record_uid = ?")
            .bind(program_uid)
            .fetch_optional(pool)
            .await?;
    if known.is_none() {
        return Err(StoreError::Protocol(
            "no such Karma Program on this Cell".into(),
        ));
    }
    if executes {
        sqlx::query("DELETE FROM karma_program_execution WHERE program_uid = ?")
            .bind(program_uid)
            .execute(pool)
            .await?;
        return Ok(());
    }
    let note = note.map(str::trim).filter(|note| !note.is_empty());
    sqlx::query(
        "INSERT INTO karma_program_execution (program_uid, executes, note, updated_at)
         VALUES (?, 0, ?, ?)
         ON CONFLICT(program_uid) DO UPDATE SET
             executes = 0, note = excluded.note, updated_at = excluded.updated_at",
    )
    .bind(program_uid)
    .bind(note)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ProgramExecutionRow>, StoreError> {
    let rows = sqlx::query(
        "SELECT program.record_uid, record.slug,
                execution.executes AS executes, execution.note, execution.updated_at
         FROM karma_program program
         JOIN record ON record.uid = program.record_uid
         LEFT JOIN karma_program_execution execution
                ON execution.program_uid = program.record_uid
         WHERE record.deleted_at IS NULL
         ORDER BY record.slug, program.record_uid",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| ProgramExecutionRow {
            program_uid: row.get("record_uid"),
            slug: row.get("slug"),
            executes: row.get::<Option<i64>, _>("executes") != Some(0),
            note: row.get("note"),
            updated_at: row.get("updated_at"),
        })
        .collect())
}
