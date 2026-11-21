use crate::{
    StoreError,
    exact::{read_decimal, zero},
    records::{NewRecord, create},
};
use nucleus::RecordKind;
use sqlx::{Row, SqlitePool, query};
use std::fmt;

/// Source and schedule supplied for a new signal.
pub struct NewSignal<'a> {
    /// Record slug.
    pub slug: &'a str,
    /// Record title.
    pub head: &'a str,
    /// Category used to select a sampler.
    pub source_kind: &'a str,
    /// Sampler configuration.
    pub source: &'a str,
    /// Sampling schedule.
    pub schedule: &'a str,
}

impl fmt::Debug for NewSignal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewSignal")
            .field("slug", &self.slug)
            .field("source_kind", &self.source_kind)
            .finish_non_exhaustive()
    }
}

/// Creates a signal Record and saves its source configuration.
///
/// # Errors
///
/// Returns an error if the Record or signal configuration cannot be stored.
pub async fn create_signal(pool: &SqlitePool, new: NewSignal<'_>) -> Result<String, StoreError> {
    let rec = create(
        pool,
        NewRecord {
            slug: Some(new.slug),
            kind: RecordKind::Signal,
            head: new.head,
            body: "",
            quantity: zero(),
        },
    )
    .await?;
    query("INSERT INTO signal (record_uid, source_kind, source, schedule) VALUES (?, ?, ?, ?)")
        .bind(&rec.uid)
        .bind(new.source_kind)
        .bind(new.source)
        .bind(new.schedule)
        .execute(pool)
        .await?;
    Ok(rec.uid)
}

/// Stored signal configuration and current sample.
#[derive(Debug, Clone)]
pub struct SignalRow {
    /// Signal Record UID.
    pub record_uid: String,
    /// Actor associated with the signal.
    pub actor_uid: Option<String>,
    /// Category used to select a sampler.
    pub source_kind: String,
    /// Sampler configuration.
    pub source: String,
    /// Sampling schedule.
    pub schedule: String,
    /// Timestamp of the last sample, if present.
    pub last_sampled_at: Option<String>,
    /// Current Record quantity as a floating-point value.
    pub current_value: f64,
}

/// Reads live signals outside the stream-backed signal path.
///
/// # Errors
///
/// Returns an error if the query or quantity decoding fails.
pub async fn list_signals(pool: &SqlitePool) -> Result<Vec<SignalRow>, StoreError> {
    query(
        "SELECT s.record_uid, s.actor_uid, s.source_kind, s.source, s.schedule, s.last_sampled_at, r.quantity_mantissa, r.quantity_scale
         FROM signal s JOIN record r ON r.uid = s.record_uid WHERE r.deleted_at IS NULL AND NOT EXISTS (SELECT 1 FROM signal_stream v WHERE v.signal_uid = s.record_uid)",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| {
        Ok(SignalRow {
            record_uid: r.try_get("record_uid")?,
            actor_uid: r.try_get("actor_uid")?,
            source_kind: r.try_get("source_kind")?,
            source: r.try_get("source")?,
            schedule: r.try_get("schedule")?,
            last_sampled_at: r.try_get("last_sampled_at")?,
            current_value: read_decimal(&r, "quantity")?.to_f64(),
        })
    })
    .collect()
}
