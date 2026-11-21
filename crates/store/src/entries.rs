//! Store queries and atomic changes for captured entries and their revision history.

#[cfg(test)]
mod tests;

use std::fmt::{Debug, Formatter, Result as FmtResult};

use chrono::{DateTime, Utc};
use nucleus::{DecimalValue, new_uid};
use sqlx::{
    Row, Sqlite, SqliteConnection, SqliteExecutor, SqlitePool, Transaction, query,
    sqlite::SqliteRow,
};

use crate::{
    StoreError,
    exact::{decimal_columns, parse_decimal},
    facts::instant,
    write_tx,
};

/// State of an entry that still contributes its amount.
pub const STATE_APPLIED: &str = "applied";
/// State of an entry whose amount was voided.
pub const STATE_VOID: &str = "void";

const BY_UID: &str = "SELECT * FROM entry WHERE uid = ?";
const BY_REQUEST: &str = "SELECT e.* FROM entry_revision r
           JOIN entry e ON e.uid = r.entry_uid
          WHERE r.request_id = ?";

/// Current state of a captured entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Entry UID.
    pub uid: String,
    /// UID of the Record that owns the entry.
    pub record_uid: String,
    /// Exact amount from the current revision.
    pub amount: DecimalValue,
    /// Optional note from the current revision.
    pub note: Option<String>,
    /// Effective instant as a UTC RFC 3339 timestamp.
    pub occurred_at: String,
    /// Current persisted state.
    pub state: String,
    /// Current revision number, starting at one.
    pub revision: i64,
    /// UID of the associated Fact, if recorded.
    pub fact_uid: Option<String>,
    /// UID of the actor who captured the entry, if recorded.
    pub actor_uid: Option<String>,
    /// Capture timestamp in UTC.
    pub created_at: String,
    /// Latest change timestamp in UTC.
    pub updated_at: String,
}

impl Entry {
    /// Returns whether the entry was voided.
    #[must_use]
    pub fn is_void(&self) -> bool {
        self.state == STATE_VOID
    }
}

/// Persisted history row from a capture, correction, or void.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRevision {
    /// Revision row UID.
    pub uid: String,
    /// UID of the entry this revision changed.
    pub entry_uid: String,
    /// Revision number within the entry.
    pub revision: i64,
    /// Persisted change kind.
    pub kind: String,
    /// Exact amount recorded in this revision.
    pub amount: DecimalValue,
    /// Optional note recorded in this revision.
    pub note: Option<String>,
    /// Effective instant in UTC.
    pub occurred_at: String,
    /// UID of the Fact applied by this revision, if any.
    pub fact_uid: Option<String>,
    /// UID of the compensation Fact, if any.
    pub compensated_fact_uid: Option<String>,
    /// Idempotency key for the change.
    pub request_id: String,
    /// UID of the actor who made the change, if recorded.
    pub actor_uid: Option<String>,
    /// Change timestamp in UTC.
    pub at: String,
}

/// Result of a new or previously recorded change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryCommit {
    /// Entry state committed by this request.
    Committed(Entry),
    /// Current state of the entry associated with a previous request.
    Replayed(Entry),
}

impl EntryCommit {
    /// Borrows the entry returned by either outcome.
    #[must_use]
    pub fn entry(&self) -> &Entry {
        match self {
            Self::Committed(entry) | Self::Replayed(entry) => entry,
        }
    }

    /// Returns whether this request was already recorded.
    #[must_use]
    pub fn was_replayed(&self) -> bool {
        matches!(self, Self::Replayed(_))
    }
}

fn protocol(message: &str) -> StoreError {
    StoreError::Protocol(message.into())
}

fn amount(row: &SqliteRow) -> Result<DecimalValue, StoreError> {
    parse_decimal(
        row.try_get("amount_mantissa")?,
        row.try_get("amount_scale")?,
    )
}

fn map_entry(row: &SqliteRow) -> Result<Entry, StoreError> {
    Ok(Entry {
        uid: row.try_get("uid")?,
        record_uid: row.try_get("record_uid")?,
        amount: amount(row)?,
        note: row.try_get("note")?,
        occurred_at: row.try_get("occurred_at")?,
        state: row.try_get("state")?,
        revision: row.try_get("revision")?,
        fact_uid: row.try_get("fact_uid")?,
        actor_uid: row.try_get("actor_uid")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn lookup<'connection>(
    executor: impl SqliteExecutor<'connection>,
    sql: &'static str,
    key: &'connection str,
) -> Result<Option<Entry>, StoreError> {
    query(sql)
        .bind(key)
        .fetch_optional(executor)
        .await?
        .as_ref()
        .map(map_entry)
        .transpose()
}

/// Loads the entry with the given UID.
/// # Errors
/// Returns an error if the query or row decoding fails.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Entry>, StoreError> {
    lookup(pool, BY_UID, uid).await
}

/// Finds an entry currently associated with a Fact.
/// # Errors
/// Returns an error if the query or row decoding fails.
pub async fn for_fact(pool: &SqlitePool, fact_uid: &str) -> Result<Option<Entry>, StoreError> {
    lookup(
        pool,
        "SELECT * FROM entry WHERE fact_uid = ? LIMIT 1",
        fact_uid,
    )
    .await
}

/// Lists entries newest effective timestamp first, breaking ties by capture time.
/// A negative limit leaves the result unbounded.
/// # Errors
/// Returns an error if the query or row decoding fails.
pub async fn list_all(pool: &SqlitePool, limit: i64) -> Result<Vec<Entry>, StoreError> {
    let rows = query("SELECT * FROM entry ORDER BY occurred_at DESC, created_at DESC LIMIT ?")
        .bind(limit)
        .fetch_all(pool)
        .await?;
    rows.iter().map(map_entry).collect()
}

/// Lists every saved change for an entry in revision order.
/// # Errors
/// Returns an error if the query or row decoding fails.
pub async fn history(pool: &SqlitePool, entry_uid: &str) -> Result<Vec<EntryRevision>, StoreError> {
    let rows = query("SELECT * FROM entry_revision WHERE entry_uid = ? ORDER BY revision")
        .bind(entry_uid)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(EntryRevision {
                uid: row.try_get("uid")?,
                entry_uid: row.try_get("entry_uid")?,
                revision: row.try_get("revision")?,
                kind: row.try_get("kind")?,
                amount: amount(row)?,
                note: row.try_get("note")?,
                occurred_at: row.try_get("occurred_at")?,
                fact_uid: row.try_get("fact_uid")?,
                compensated_fact_uid: row.try_get("compensated_fact_uid")?,
                request_id: row.try_get("request_id")?,
                actor_uid: row.try_get("actor_uid")?,
                at: row.try_get("at")?,
            })
        })
        .collect()
}

/// Finds the current entry associated with an already recorded request.
/// # Errors
/// Returns an error if the query or row decoding fails.
pub async fn replayed(pool: &SqlitePool, request_id: &str) -> Result<Option<Entry>, StoreError> {
    lookup(pool, BY_REQUEST, request_id).await
}

/// Inputs for capturing an entry on a Record.
pub struct NewEntry<'a> {
    /// UID of the owning Record.
    pub record_uid: &'a str,
    /// Exact amount to capture.
    pub amount: DecimalValue,
    /// Optional note to record.
    pub note: Option<&'a str>,
    /// Effective time of the entry.
    pub occurred_at: DateTime<Utc>,
    /// UID of the Fact applied for the capture.
    pub fact_uid: &'a str,
    /// Idempotency key.
    pub request_id: &'a str,
    /// UID of the actor making the capture, if recorded.
    pub actor_uid: Option<&'a str>,
}

impl Debug for NewEntry<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("NewEntry")
            .field("record_uid", &self.record_uid)
            .finish_non_exhaustive()
    }
}

/// Captures an entry and its first revision in one transaction.
/// # Errors
/// Returns an error for a blank request key, a database failure, or a row
/// that cannot be read back.
pub async fn create(
    pool: &SqlitePool,
    input: NewEntry<'_>,
    now: DateTime<Utc>,
) -> Result<EntryCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("entry needs a request id"));
    }
    // Hold the write lock from the replay check through the revision insert.
    let mut tx = write_tx(pool).await?;
    if let Some(existing) = lookup(&mut *tx, BY_REQUEST, input.request_id).await? {
        return Ok(EntryCommit::Replayed(existing));
    }

    let uid = new_uid("en");
    let at = instant(now);
    let occurred_at = instant(input.occurred_at);
    let (mantissa, scale) = decimal_columns(input.amount);

    query(
        "INSERT INTO entry
           (uid, record_uid, amount_mantissa, amount_scale, note, occurred_at,
            state, revision, fact_uid, actor_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.record_uid)
    .bind(&mantissa)
    .bind(scale)
    .bind(input.note)
    .bind(&occurred_at)
    .bind(STATE_APPLIED)
    .bind(input.fact_uid)
    .bind(input.actor_uid)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            entry_uid: &uid,
            revision: 1,
            kind: "created",
            mantissa: &mantissa,
            scale,
            note: input.note,
            occurred_at: &occurred_at,
            fact_uid: Some(input.fact_uid),
            compensated_fact_uid: None,
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    commit_entry(tx, &uid, "entry vanished after insert").await
}

/// Inputs for correcting the current revision of an entry.
pub struct ReviseEntry<'a> {
    /// UID of the entry to correct.
    pub entry_uid: &'a str,
    /// Revision observed by the caller.
    pub expected_revision: i64,
    /// Exact replacement amount.
    pub amount: DecimalValue,
    /// Optional replacement note.
    pub note: Option<&'a str>,
    /// Effective time of the corrected entry.
    pub occurred_at: DateTime<Utc>,
    /// UID of the compensation Fact, if any.
    pub compensated_fact_uid: Option<&'a str>,
    /// UID of the new applied Fact, if replacing the existing one.
    pub replacement_fact_uid: Option<&'a str>,
    /// Idempotency key.
    pub request_id: &'a str,
    /// UID of the actor making the correction, if recorded.
    pub actor_uid: Option<&'a str>,
}

impl Debug for ReviseEntry<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ReviseEntry")
            .field("entry_uid", &self.entry_uid)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

/// Corrects an entry and appends the next revision in one transaction.
/// # Errors
/// Returns an error for a blank key, a missing, voided, or stale entry, a
/// revision outside the stored range, or a database failure.
pub async fn revise(
    pool: &SqlitePool,
    input: ReviseEntry<'_>,
    now: DateTime<Utc>,
) -> Result<EntryCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("entry revision needs a request id"));
    }
    let mut tx = write_tx(pool).await?;
    if let Some(existing) = lookup(&mut *tx, BY_REQUEST, input.request_id).await? {
        return Ok(EntryCommit::Replayed(existing));
    }
    let (current, revision) = next_revision(
        &mut tx,
        input.entry_uid,
        input.expected_revision,
        "a voided entry cannot be revised",
    )
    .await?;
    let at = instant(now);
    let occurred_at = instant(input.occurred_at);
    let (mantissa, scale) = decimal_columns(input.amount);
    let live_fact = input.replacement_fact_uid.or(current.fact_uid.as_deref());

    query(
        "UPDATE entry
            SET amount_mantissa = ?, amount_scale = ?, note = ?, occurred_at = ?,
                revision = ?, fact_uid = ?, updated_at = ?
          WHERE uid = ? AND revision = ?",
    )
    .bind(&mantissa)
    .bind(scale)
    .bind(input.note)
    .bind(&occurred_at)
    .bind(revision)
    .bind(live_fact)
    .bind(&at)
    .bind(input.entry_uid)
    .bind(input.expected_revision)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            entry_uid: input.entry_uid,
            revision,
            kind: "revised",
            mantissa: &mantissa,
            scale,
            note: input.note,
            occurred_at: &occurred_at,
            fact_uid: input.replacement_fact_uid,
            compensated_fact_uid: input.compensated_fact_uid,
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    commit_entry(tx, input.entry_uid, "entry vanished after revision").await
}

/// Inputs for voiding an entry.
pub struct VoidEntry<'a> {
    /// UID of the entry to void.
    pub entry_uid: &'a str,
    /// Revision observed by the caller.
    pub expected_revision: i64,
    /// UID of the compensation Fact, if any.
    pub compensated_fact_uid: Option<&'a str>,
    /// Idempotency key.
    pub request_id: &'a str,
    /// UID of the actor voiding the entry, if recorded.
    pub actor_uid: Option<&'a str>,
}

impl Debug for VoidEntry<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("VoidEntry")
            .field("entry_uid", &self.entry_uid)
            .field("expected_revision", &self.expected_revision)
            .finish_non_exhaustive()
    }
}

/// Marks an entry void and appends the next revision in one transaction.
/// # Errors
/// Returns an error for a blank key, a missing, voided, or stale entry, a
/// revision outside the stored range, or a database failure.
pub async fn void(
    pool: &SqlitePool,
    input: VoidEntry<'_>,
    now: DateTime<Utc>,
) -> Result<EntryCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("voiding an entry needs a request id"));
    }
    let mut tx = write_tx(pool).await?;
    if let Some(existing) = lookup(&mut *tx, BY_REQUEST, input.request_id).await? {
        return Ok(EntryCommit::Replayed(existing));
    }
    let (current, revision) = next_revision(
        &mut tx,
        input.entry_uid,
        input.expected_revision,
        "entry is already void",
    )
    .await?;
    let at = instant(now);
    let (mantissa, scale) = decimal_columns(current.amount);

    query(
        "UPDATE entry SET state = ?, revision = ?, updated_at = ?
          WHERE uid = ? AND revision = ?",
    )
    .bind(STATE_VOID)
    .bind(revision)
    .bind(&at)
    .bind(input.entry_uid)
    .bind(input.expected_revision)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            entry_uid: input.entry_uid,
            revision,
            kind: "voided",
            mantissa: &mantissa,
            scale,
            note: current.note.as_deref(),
            occurred_at: &current.occurred_at,
            fact_uid: None,
            compensated_fact_uid: input.compensated_fact_uid,
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    commit_entry(tx, input.entry_uid, "entry vanished after voiding").await
}

async fn next_revision(
    connection: &mut SqliteConnection,
    uid: &str,
    expected_revision: i64,
    voided: &str,
) -> Result<(Entry, i64), StoreError> {
    let entry = lookup(connection, BY_UID, uid)
        .await?
        .ok_or_else(|| protocol("no such entry"))?;
    if entry.is_void() {
        return Err(protocol(voided));
    }
    if entry.revision != expected_revision {
        return Err(protocol("entry was changed by someone else"));
    }
    let revision = expected_revision
        .checked_add(1)
        .ok_or_else(|| protocol("entry revision exceeds SQLite integer range"))?;
    Ok((entry, revision))
}

async fn commit_entry(
    mut tx: Transaction<'_, Sqlite>,
    uid: &str,
    vanished: &str,
) -> Result<EntryCommit, StoreError> {
    let entry = lookup(&mut *tx, BY_UID, uid)
        .await?
        .ok_or_else(|| protocol(vanished))?;
    tx.commit().await?;
    Ok(EntryCommit::Committed(entry))
}

struct RevisionRow<'a> {
    entry_uid: &'a str,
    revision: i64,
    kind: &'a str,
    mantissa: &'a str,
    scale: i64,
    note: Option<&'a str>,
    occurred_at: &'a str,
    fact_uid: Option<&'a str>,
    compensated_fact_uid: Option<&'a str>,
    request_id: &'a str,
    actor_uid: Option<&'a str>,
    at: &'a str,
}

async fn insert_revision(
    tx: &mut Transaction<'_, Sqlite>,
    row: RevisionRow<'_>,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO entry_revision
           (uid, entry_uid, revision, kind, amount_mantissa, amount_scale, note,
            occurred_at, fact_uid, compensated_fact_uid, request_id, actor_uid, at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(new_uid("enr"))
    .bind(row.entry_uid)
    .bind(row.revision)
    .bind(row.kind)
    .bind(row.mantissa)
    .bind(row.scale)
    .bind(row.note)
    .bind(row.occurred_at)
    .bind(row.fact_uid)
    .bind(row.compensated_fact_uid)
    .bind(row.request_id)
    .bind(row.actor_uid)
    .bind(row.at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
