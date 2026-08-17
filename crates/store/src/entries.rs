//! Entries: the editable, voidable handle over an authored quantity change.
//!
//! A captured change is a signed Fact plus a classification assertion, and
//! neither can be edited — the Fact is hash-chained, the classification is an
//! assertion *about* it. So "I typed 15 instead of 150" needs somewhere to live
//! that is allowed to change. That is this module.
//!
//! Editing never rewrites history: a revision compensates the old Fact and the
//! caller appends a replacement, so the Ledger keeps both and the chain stays
//! intact. What changes here is only which Fact is currently the live one.
//!
//! Nothing here is domain-specific. A mistyped expense, a miscounted stock
//! take, and a wrongly logged training set are the same problem: an authored
//! change that turned out to be wrong.
//!
//! **Classification is not stored here.** It has exactly one owner,
//! `fact_concept`, and duplicating it would let the two disagree the moment a
//! change is re-tagged. Read it by joining through `fact_uid`.

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::exact::{decimal_columns, read_decimal};
use crate::facts::instant;

fn protocol(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.to_string())
}

pub const STATE_APPLIED: &str = "applied";
pub const STATE_VOID: &str = "void";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub uid: String,
    pub record_uid: String,
    pub amount: DecimalValue,
    pub note: Option<String>,
    pub occurred_at: String,
    pub state: String,
    pub revision: i64,
    pub fact_uid: Option<String>,
    pub actor_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Entry {
    pub fn is_void(&self) -> bool {
        self.state == STATE_VOID
    }
}

/// One step in an entry's audit trail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRevision {
    pub uid: String,
    pub entry_uid: String,
    pub revision: i64,
    pub kind: String,
    pub amount: DecimalValue,
    pub note: Option<String>,
    pub occurred_at: String,
    pub fact_uid: Option<String>,
    pub compensated_fact_uid: Option<String>,
    pub request_id: String,
    pub actor_uid: Option<String>,
    pub at: String,
}

/// Whether a call did the work or found that it already had.
///
/// `Replayed` is not an error and not a no-op — it is the *same answer* the
/// first call gave. A retried request that returned an error would push callers
/// toward retrying differently, which is how one hiccup becomes two applied changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryCommit {
    Committed(Entry),
    Replayed(Entry),
}

impl EntryCommit {
    pub fn entry(&self) -> &Entry {
        match self {
            Self::Committed(entry) | Self::Replayed(entry) => entry,
        }
    }

    pub fn was_replayed(&self) -> bool {
        matches!(self, Self::Replayed(_))
    }
}

fn map_entry(row: &sqlx::sqlite::SqliteRow) -> Result<Entry, StoreError> {
    Ok(Entry {
        uid: row.get("uid"),
        record_uid: row.get("record_uid"),
        amount: read_decimal(row, "amount")?,
        note: row.get("note"),
        occurred_at: row.get("occurred_at"),
        state: row.get("state"),
        revision: row.get("revision"),
        fact_uid: row.get("fact_uid"),
        actor_uid: row.get("actor_uid"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Entry>, StoreError> {
    let row = sqlx::query("SELECT * FROM entry WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_entry).transpose()
}

/// The entry that currently owns a Fact, if any. This is how the write path
/// tells "x/// entry that should be revised or voided instead".
pub async fn for_fact(pool: &SqlitePool, fact_uid: &str) -> Result<Option<Entry>, StoreError> {
    let row = sqlx::query("SELECT * FROM entry WHERE fact_uid = ? LIMIT 1")
        .bind(fact_uid)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_entry).transpose()
}

/// Every entry, newest change first. Voided ones are included: an append-only
/// Ledger has no delete, and a surface that hid them would make a correction
/// look like a disappearance.
pub async fn list_all(pool: &SqlitePool, limit: i64) -> Result<Vec<Entry>, StoreError> {
    let rows =
        sqlx::query("SELECT * FROM entry ORDER BY occurred_at DESC, created_at DESC LIMIT ?")
            .bind(limit)
            .fetch_all(pool)
            .await?;
    rows.iter().map(map_entry).collect()
}

pub async fn history(pool: &SqlitePool, entry_uid: &str) -> Result<Vec<EntryRevision>, StoreError> {
    let rows = sqlx::query("SELECT * FROM entry_revision WHERE entry_uid = ? ORDER BY revision")
        .bind(entry_uid)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(EntryRevision {
                uid: row.get("uid"),
                entry_uid: row.get("entry_uid"),
                revision: row.get("revision"),
                kind: row.get("kind"),
                amount: read_decimal(row, "amount")?,
                note: row.get("note"),
                occurred_at: row.get("occurred_at"),
                fact_uid: row.get("fact_uid"),
                compensated_fact_uid: row.get("compensated_fact_uid"),
                request_id: row.get("request_id"),
                actor_uid: row.get("actor_uid"),
                at: row.get("at"),
            })
        })
        .collect()
}

/// The entry a request already produced, if this request has been seen.
///
/// **Callers must consult this before doing any Ledger work.** The engine
/// appends Facts outside this module's transaction, so a replay detected only
/// at insert time would already have appended a duplicate compensating Fact —
/// the quantity moved twice, and the error afterwards would not put it back.
pub async fn replayed(pool: &SqlitePool, request_id: &str) -> Result<Option<Entry>, StoreError> {
    let row = sqlx::query(
        "SELECT e.* FROM entry_revision r
           JOIN entry e ON e.uid = r.entry_uid
          WHERE r.request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(map_entry).transpose()
}

pub struct NewEntry<'a> {
    pub record_uid: &'a str,
    pub amount: DecimalValue,
    pub note: Option<&'a str>,
    pub occurred_at: DateTime<Utc>,
    /// The Fact that carried the amount. Appended by the caller, because only
    /// the engine can seal and sign one.
    pub fact_uid: &'a str,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

pub async fn create(
    pool: &SqlitePool,
    input: NewEntry<'_>,
    now: DateTime<Utc>,
) -> Result<EntryCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("entry needs a request id"));
    }
    if let Some(existing) = replayed(pool, input.request_id).await? {
        return Ok(EntryCommit::Replayed(existing));
    }

    let uid = nucleus::new_uid("en");
    let at = instant(now);
    let occurred_at = instant(input.occurred_at);
    let (mantissa, scale) = decimal_columns(input.amount);

    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
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
    tx.commit().await?;

    get(pool, &uid)
        .await?
        .map(EntryCommit::Committed)
        .ok_or_else(|| protocol("entry vanished after insert"))
}

pub struct ReviseEntry<'a> {
    pub entry_uid: &'a str,
    pub expected_revision: i64,
    pub amount: DecimalValue,
    pub note: Option<&'a str>,
    pub occurred_at: DateTime<Utc>,
    /// The Fact reversing the previous amount, and the Fact carrying the new
    /// one. Both `None` when nothing about the quantity changed — a note-only
    /// edit still earns a revision and an audit row, but moves nothing.
    pub compensated_fact_uid: Option<&'a str>,
    pub replacement_fact_uid: Option<&'a str>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

pub async fn revise(
    pool: &SqlitePool,
    input: ReviseEntry<'_>,
    now: DateTime<Utc>,
) -> Result<EntryCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("entry revision needs a request id"));
    }
    if let Some(existing) = replayed(pool, input.request_id).await? {
        return Ok(EntryCommit::Replayed(existing));
    }
    let current = get(pool, input.entry_uid)
        .await?
        .ok_or_else(|| protocol("no such entry"))?;
    if current.is_void() {
        return Err(protocol("a voided entry cannot be revised"));
    }
    if current.revision != input.expected_revision {
        return Err(protocol("entry was changed by someone else"));
    }

    let revision = current.revision + 1;
    let at = instant(now);
    let occurred_at = instant(input.occurred_at);
    let (mantissa, scale) = decimal_columns(input.amount);
    // A note-only edit moves no quantity, so the live Fact stays the one already
    // carrying this entry's amount. Blanking it would orphan the entry from
    // its own Ledger entry.
    let live_fact = input
        .replacement_fact_uid
        .map(str::to_string)
        .or_else(|| current.fact_uid.clone());

    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
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
    .bind(live_fact.as_deref())
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
    tx.commit().await?;

    get(pool, input.entry_uid)
        .await?
        .map(EntryCommit::Committed)
        .ok_or_else(|| protocol("entry vanished after revision"))
}

pub struct VoidEntry<'a> {
    pub entry_uid: &'a str,
    pub expected_revision: i64,
    /// The Fact returning the amount. `None` only when the entry carried no
    /// quantity to return.
    pub compensated_fact_uid: Option<&'a str>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

pub async fn void(
    pool: &SqlitePool,
    input: VoidEntry<'_>,
    now: DateTime<Utc>,
) -> Result<EntryCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("voiding an entry needs a request id"));
    }
    if let Some(existing) = replayed(pool, input.request_id).await? {
        return Ok(EntryCommit::Replayed(existing));
    }
    let current = get(pool, input.entry_uid)
        .await?
        .ok_or_else(|| protocol("no such entry"))?;
    if current.is_void() {
        return Err(protocol("entry is already void"));
    }
    if current.revision != input.expected_revision {
        return Err(protocol("entry was changed by someone else"));
    }

    let revision = current.revision + 1;
    let at = instant(now);
    let (mantissa, scale) = decimal_columns(current.amount);

    let mut tx = crate::write_tx(pool).await?;
    // `fact_uid` deliberately keeps pointing at the compensated Fact: that is
    // still the change this entry describes, and the compensation is a
    // separate Ledger entry rather than a replacement for it.
    sqlx::query(
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
    tx.commit().await?;

    get(pool, input.entry_uid)
        .await?
        .map(EntryCommit::Committed)
        .ok_or_else(|| protocol("entry vanished after voiding"))
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
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: RevisionRow<'_>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO entry_revision
           (uid, entry_uid, revision, kind, amount_mantissa, amount_scale, note,
            occurred_at, fact_uid, compensated_fact_uid, request_id, actor_uid, at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("enr"))
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
