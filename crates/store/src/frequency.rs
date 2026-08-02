//! Frequencies: named beats, declared once and read by any condition.
//!
//! A Frequency is a slug and a step. A condition reads it as `freq(@daily)`,
//! and every rule naming it shares this one definition rather than each
//! restating the schedule and drifting apart the first time one is edited.
//!
//! ## Beats are derived, never stored
//!
//! [`nucleus::karma::Cadence`] is pure: given the anchor and the step it
//! enumerates dates with no database and no cursor. That one function answers
//! both questions a Frequency is ever asked —
//!
//! - *when does this fire next* — [`Cadence::next_on_or_after`]
//! - *what do the next twelve months look like* — [`Cadence::between`]
//!
//! — which is why a calendar and a graph need no storage of their own, and why
//! there is deliberately no beat table here to keep in sync with anything.

use chrono::{DateTime, Utc};
use nucleus::karma::{Cadence, CadenceStep};
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::facts::instant;

fn protocol(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frequency {
    pub uid: String,
    /// What a condition calls it: the `daily` in `freq(@daily)`.
    pub slug: String,
    pub head: String,
    pub every: CadenceStep,
    pub anchor_at: String,
    pub actor_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Frequency {
    /// The beat as a cadence, which is the only form anything downstream wants.
    ///
    /// A Frequency has no bound: it is a rhythm, not a run of dates. Rules stop
    /// by being paused or deleted, and a beat nobody reads simply costs
    /// nothing.
    pub fn cadence(&self) -> Cadence {
        Cadence::every(self.every)
    }

    pub fn anchor(&self) -> Result<DateTime<Utc>, StoreError> {
        DateTime::parse_from_rfc3339(&self.anchor_at)
            .map(|at| at.with_timezone(&Utc))
            .map_err(|_| protocol("frequency anchor is not an instant"))
    }
}

pub struct NewFrequency<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub every: CadenceStep,
    pub anchor_at: DateTime<Utc>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

fn row_to_frequency(row: &sqlx::sqlite::SqliteRow) -> Result<Frequency, StoreError> {
    let every_json: String = row.try_get("every_json")?;
    let every: CadenceStep = serde_json::from_str(&every_json)
        .map_err(|_| protocol("stored frequency step is unreadable"))?;
    Ok(Frequency {
        uid: row.try_get("uid")?,
        slug: row.try_get("slug")?,
        head: row.try_get("head")?,
        every,
        anchor_at: row.try_get("anchor_at")?,
        actor_uid: row.try_get("actor_uid")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Declare a frequency.
///
/// Idempotent on `request_id`: a retry returns the frequency the first attempt
/// made rather than declaring a second one under a new uid.
pub async fn create(
    pool: &SqlitePool,
    new: NewFrequency<'_>,
    now: DateTime<Utc>,
) -> Result<Frequency, StoreError> {
    let slug = new.slug.trim().trim_start_matches('@');
    if slug.is_empty() {
        return Err(protocol("a frequency needs a name to be read by"));
    }
    // The slug is what a condition types. Letting it hold anything the
    // expression lexer treats as an operator would make a reading that cannot
    // be written down.
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(protocol(
            "a frequency name may hold letters, digits, dot, dash and underscore",
        ));
    }
    // A step that advances nothing would name a beat that never comes.
    if new.every.is_zero() {
        return Err(protocol("a frequency needs a component to repeat by"));
    }

    if let Some(existing) = by_request(pool, new.request_id).await? {
        return Ok(existing);
    }

    let every_json = serde_json::to_string(&new.every)
        .map_err(|_| protocol("that frequency step cannot be stored"))?;
    let uid = nucleus::new_uid("freq");
    let at = instant(now);
    let anchor = instant(new.anchor_at);
    let head = if new.head.trim().is_empty() {
        slug
    } else {
        new.head.trim()
    };

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO frequency (uid, slug, head, every_json, anchor_at, actor_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(slug)
    .bind(head)
    .bind(&every_json)
    .bind(&anchor)
    .bind(new.actor_uid)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await
    .map_err(|error| match error {
        sqlx::Error::Database(ref db) if db.message().contains("UNIQUE") => {
            protocol("a frequency is already called that")
        }
        other => other,
    })?;

    sqlx::query(
        "INSERT INTO frequency_revision (uid, frequency_uid, kind, head, every_json, anchor_at, request_id, actor_uid, at)
         VALUES (?, ?, 'created', ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("freqrev"))
    .bind(&uid)
    .bind(head)
    .bind(&every_json)
    .bind(&anchor)
    .bind(new.request_id)
    .bind(new.actor_uid)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    get(pool, &uid)
        .await?
        .ok_or_else(|| protocol("the declared frequency is missing"))
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Frequency>, StoreError> {
    let row = sqlx::query("SELECT * FROM frequency WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_frequency).transpose()
}

/// Resolve what a condition wrote: `freq(@daily)` finds the frequency called
/// `daily`. A uid is accepted too, so a caller holding one need not look it up.
pub async fn resolve(pool: &SqlitePool, name: &str) -> Result<Option<Frequency>, StoreError> {
    let name = name.trim().trim_start_matches('@');
    let row = sqlx::query("SELECT * FROM frequency WHERE slug = ? OR uid = ?")
        .bind(name)
        .bind(name)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(row_to_frequency).transpose()
}

pub async fn all(pool: &SqlitePool) -> Result<Vec<Frequency>, StoreError> {
    let rows = sqlx::query("SELECT * FROM frequency ORDER BY slug")
        .fetch_all(pool)
        .await?;
    rows.iter().map(row_to_frequency).collect()
}

async fn by_request(pool: &SqlitePool, request_id: &str) -> Result<Option<Frequency>, StoreError> {
    let row = sqlx::query(
        "SELECT f.* FROM frequency f
         JOIN frequency_revision r ON r.frequency_uid = f.uid
         WHERE r.request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(row_to_frequency).transpose()
}

/// Forget a frequency.
///
/// Refused while a rule still reads it: a condition naming a beat that no
/// longer exists would evaluate to nothing and quietly stop firing, which reads
/// exactly like a rule whose reading is simply false.
pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    let frequency = get(pool, uid)
        .await?
        .ok_or_else(|| protocol("no such frequency"))?;
    let needle = format!("%@{}%", frequency.slug);
    let readers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM recurrence WHERE condition_src LIKE ?")
            .bind(&needle)
            .fetch_one(pool)
            .await?;
    if readers > 0 {
        return Err(protocol("a rule still reads that frequency"));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM frequency_revision WHERE frequency_uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM frequency WHERE uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
