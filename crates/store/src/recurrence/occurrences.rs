use std::{borrow::Cow, collections::HashSet, vec::IntoIter};

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use sqlx::{Row, SqlitePool, query, query_scalar};

use super::{Recurrence, protocol};
use crate::{StoreError, exact::read_decimal, facts::instant, frequency};

/// Builds the idempotency key for a rule and its exact occurrence timestamp.
pub fn occurrence_request_id(recurrence_uid: &str, due_at: DateTime<Utc>) -> String {
    format!("{recurrence_uid}:{}", instant(due_at))
}

/// Current status of a derived occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceState {
    /// Future occurrence.
    Planned,
    /// Occurrence at or before the current time.
    Due,
    /// The occurrence has already been applied.
    Applied,
    /// The occurrence is marked to be skipped.
    Skipped,
}

impl OccurrenceState {
    /// Returns the stable status name used by callers.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Due => "due",
            Self::Applied => "applied",
            Self::Skipped => "skipped",
        }
    }
}

/// A date derived from a rule and annotated with stored activity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    /// Rule that produced the date.
    pub recurrence_uid: String,
    /// Intended execution time.
    pub due_at: DateTime<Utc>,
    /// Status after considering stored applications and skips.
    pub state: OccurrenceState,
    /// Applied entry identity, when the application produced one.
    pub entry_uid: Option<String>,
    /// Applied amount or the rule's declared change, when known.
    pub amount: Option<DecimalValue>,
}

/// Records an explicit skip if one is not already present.
///
/// # Errors
/// Returns an error if the skip cannot be written.
pub async fn skip(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
    note: Option<&str>,
    actor_uid: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO recurrence_skip (recurrence_uid, due_at, note, actor_uid, at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(recurrence_uid, due_at) DO NOTHING",
    )
    .bind(recurrence_uid)
    .bind(instant(due_at))
    .bind(note)
    .bind(actor_uid)
    .bind(instant(now))
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes an explicit skip from an occurrence.
///
/// # Errors
/// Returns an error if the delete fails.
pub async fn unskip(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
) -> Result<(), StoreError> {
    query("DELETE FROM recurrence_skip WHERE recurrence_uid = ? AND due_at = ?")
        .bind(recurrence_uid)
        .bind(instant(due_at))
        .execute(pool)
        .await?;
    Ok(())
}

/// Derives rule occurrences in the requested interval and applies recorded skips and activity.
///
/// # Errors
/// Returns an error for an invalid calendar or rule definition, or if activity cannot be read.
pub async fn occurrences(
    pool: &SqlitePool,
    rule: &Recurrence,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Occurrences, StoreError> {
    let mut anchor = parse_instant(&rule.anchor_at)?;
    let mut cadence = Cow::Borrowed(&rule.cadence);
    if let Some(condition) = &rule.condition {
        let parsed = condition
            .parsed()
            .map_err(|error| protocol(error.to_string()))?;
        if let Some(token) = parsed.reads_ref().iter().find(|token| token.func == "freq")
            && let Some(frequency) = frequency::resolve(pool, &token.slug).await?
        {
            anchor = frequency.anchor()?;
            cadence = Cow::Owned(frequency.cadence());
        }
    }
    let derived = cadence
        .between(anchor, from, to)
        .map_err(|error| protocol(error.to_string()))?;
    let mut result = Occurrences {
        truncated: derived.truncated,
        dates: Vec::with_capacity(derived.len()),
    };
    if derived.is_empty() {
        return Ok(result);
    }

    let skipped = skipped_dates(pool, &rule.uid).await?;
    let declared = rule.consequences.declared_delta().copied();
    let paused = rule.is_paused();
    for due_at in derived {
        if paused && due_at > now {
            continue;
        }
        let key = instant(due_at);
        let applied = applied_entry(pool, format!("{}:{key}", rule.uid)).await?;
        let runtime_applied: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_application WHERE rule_uid = ? AND frequency_uid IS NOT NULL AND intended_at = ? AND status = 'applied')")
            .bind(&rule.uid).bind(due_at.to_rfc3339()).fetch_one(pool).await?;
        let (state, entry_uid, amount) = match applied {
            Some((uid, amount)) => (OccurrenceState::Applied, Some(uid), Some(amount)),
            None if runtime_applied => (OccurrenceState::Applied, None, declared),
            None if skipped.contains(&key) => (OccurrenceState::Skipped, None, declared),
            None if due_at <= now => (OccurrenceState::Due, None, declared),
            None => (OccurrenceState::Planned, None, declared),
        };
        result.dates.push(Occurrence {
            recurrence_uid: rule.uid.clone(),
            due_at,
            state,
            entry_uid,
            amount,
        });
    }
    Ok(result)
}

/// Derived dates and an indication that the cadence reached its search limit.
#[derive(Debug, Clone, Default)]
pub struct Occurrences {
    /// Dates in cadence order.
    pub dates: Vec<Occurrence>,
    /// Whether date derivation reached its bound.
    pub truncated: bool,
}

impl Occurrences {
    /// Returns whether the list contains no occurrences.
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty()
    }

    /// Returns the number of returned occurrences.
    pub fn len(&self) -> usize {
        self.dates.len()
    }

    /// Iterates over the returned dates in cadence order.
    pub fn iter(&self) -> impl Iterator<Item = &Occurrence> {
        self.dates.iter()
    }
}

impl IntoIterator for Occurrences {
    type Item = Occurrence;
    type IntoIter = IntoIter<Occurrence>;

    fn into_iter(self) -> Self::IntoIter {
        self.dates.into_iter()
    }
}

async fn skipped_dates(
    pool: &SqlitePool,
    recurrence_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(
        query_scalar("SELECT due_at FROM recurrence_skip WHERE recurrence_uid = ?")
            .bind(recurrence_uid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect(),
    )
}

/// Returns the entry identity for an applied occurrence, when one exists.
///
/// # Errors
/// Returns an error if the applied entry cannot be queried or decoded.
pub async fn applied(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
) -> Result<Option<String>, StoreError> {
    Ok(
        applied_entry(pool, occurrence_request_id(recurrence_uid, due_at))
            .await?
            .map(|(uid, _)| uid),
    )
}

async fn applied_entry(
    pool: &SqlitePool,
    request_id: String,
) -> Result<Option<(String, DecimalValue)>, StoreError> {
    query(
        "SELECT e.uid AS uid, e.amount_mantissa AS amount_mantissa,
                e.amount_scale AS amount_scale
           FROM entry_revision v
           JOIN entry e ON e.uid = v.entry_uid
          WHERE v.request_id = ?",
    )
    .bind(request_id)
    .try_map(|row| Ok((row.try_get("uid")?, read_decimal(&row, "amount")?)))
    .fetch_optional(pool)
    .await
}

fn parse_instant(text: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            protocol(format!(
                "recurring rule holds an unreadable timestamp: {error}"
            ))
        })
}
