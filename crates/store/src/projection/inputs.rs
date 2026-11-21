use crate::{
    StoreError,
    records::{RecordRow, RecordSelection, extension_rows, map_extensions, map_row, read_rows},
};
use serde_json::{Value, from_str};
use sqlx::{SqlitePool, query, query_scalar};
use std::collections::HashMap;

/// A consistent snapshot of manual forecast inputs.
#[derive(Debug)]
pub struct ManualInputs {
    /// Source revision read in the same transaction.
    pub revision: i64,
    /// Records in the source snapshot.
    pub records: Vec<RecordRow<Option<String>>>,
    /// Work extensions keyed by Record UID.
    pub work: HashMap<String, Value>,
}

/// Reads manual forecast inputs when the source can skip simulation.
///
/// # Errors
///
/// Returns an error on failed reads or malformed stored Record or extension data.
pub async fn manual_inputs(pool: &SqlitePool) -> Result<Option<ManualInputs>, StoreError> {
    let mut transaction = pool.begin().await?;
    let revision = query_scalar("SELECT revision FROM projection_source WHERE id = 1")
        .fetch_one(&mut *transaction)
        .await?;
    let simulation: bool = query_scalar(
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
    let runtime: Option<String> = query_scalar(
        "SELECT e.fds FROM record_extension e JOIN record r ON r.uid = e.record_uid
         WHERE r.slug = 'local-cell' AND r.kind = 'device' AND e.namespace = 'lince.karma-runtime'",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let running = runtime
        .as_deref()
        .and_then(|raw| from_str::<Value>(raw).ok())
        .and_then(|value| value.get("running")?.as_bool())
        .unwrap_or(true);
    if !running {
        transaction.rollback().await?;
        return Ok(None);
    }
    let records = read_rows(&mut *transaction, RecordSelection::All, false).await?;
    let work = extension_rows(&mut *transaction, "work").await?;
    transaction.rollback().await?;
    let records = records.into_iter().map(map_row).collect::<Result<_, _>>()?;
    Ok(Some(ManualInputs {
        revision,
        records,
        work: map_extensions(work),
    }))
}

/// Reads the current source revision.
///
/// # Errors
///
/// Returns an error if the revision row cannot be read.
pub async fn revision(pool: &SqlitePool) -> Result<i64, StoreError> {
    query_scalar("SELECT revision FROM projection_source WHERE id = 1")
        .fetch_one(pool)
        .await
}

/// Updates the runtime identity and invalidates projections when it changes.
///
/// # Errors
///
/// Returns an error if the source row cannot be read or updated.
pub async fn set_runtime(pool: &SqlitePool, runtime: &str) -> Result<(), StoreError> {
    let current: Option<String> =
        query_scalar("SELECT runtime FROM projection_source WHERE id = 1")
            .fetch_one(pool)
            .await?;
    if current.as_deref() == Some(runtime) {
        return Ok(());
    }
    query("UPDATE projection_source SET runtime = ?, revision = revision + 1 WHERE id = 1 AND runtime IS NOT ?").bind(runtime).bind(runtime).execute(pool).await?;
    Ok(())
}
