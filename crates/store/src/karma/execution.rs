//! Which Programs THIS Cell executes — the second of C7's two axes.
//!
//! The first axis is whether the Rule is synced, and that one needs no code
//! here: a Program is anchored to a Record, and Records sync. This axis is the
//! independent other half, and it is deliberately local. Executing is a
//! property of a machine, so the same synced Program can run on the always-on
//! Cell and sit dormant on the laptop without either Cell disagreeing with the
//! other about what the rule IS.
//!
//! Absence means execute. Every read here is therefore a LEFT JOIN or an
//! `IS NOT 0` rather than an equality test, and the table stays empty on an
//! Organ that never touches the setting — which is every Organ with one Cell.

use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

/// One Cell's answer for one Program, as the surface needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramExecutionRow {
    pub program_uid: String,
    pub slug: String,
    pub executes: bool,
    pub note: Option<String>,
    pub updated_at: Option<String>,
}

/// Does this Cell execute `program_uid`?
///
/// An unknown Program answers `true`. That is not a guess about a row that
/// should exist — a Program with no row here has simply never been narrowed,
/// which is the default state and the one that preserves single-Cell
/// behaviour.
pub async fn executes(pool: &SqlitePool, program_uid: &str) -> Result<bool, StoreError> {
    let off: Option<i64> =
        sqlx::query_scalar("SELECT executes FROM karma_program_execution WHERE program_uid = ?")
            .bind(program_uid)
            .fetch_optional(pool)
            .await?;
    Ok(off != Some(0))
}

/// Set (or clear) this Cell's answer.
///
/// Setting it back to `true` DELETES the row rather than storing a 1, so the
/// table holds deviations only and "has this Cell been configured at all" stays
/// answerable by its emptiness.
pub async fn set_executes(
    pool: &SqlitePool,
    program_uid: &str,
    executes: bool,
    note: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    // Refuse a uid that names no Program rather than storing a setting that
    // will never be read. The foreign key would catch it, but as an opaque
    // constraint failure the surface cannot turn into a sentence.
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

// The designation — "which Cell is the one" — used to live here and now lives
// in `store::executor`, because transfer delivery retries ask the same question
// and a namespace saying `karma` on a Transfer Record would be a lie. What
// stays here is the LOCAL axis: this machine's own answer, which never travels.

/// Every active Program with this Cell's answer beside it.
///
/// The list is built from `karma_program`, not from the settings table, so a
/// Program nobody has configured still appears — showing only the deviations
/// would mean the surface could not answer "what runs here", which is the
/// question the whole axis exists to make answerable.
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
