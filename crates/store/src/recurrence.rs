//! Recurring declarations: what is expected to happen again, and when.
//!
//! A rule states an amount, a cadence, and what the change counts as. It writes
//! no Fact and holds no total. Applying one of its dates appends an ordinary
//! entry, so a rent paid by a rule and a rent typed by hand are the same kind of
//! thing in the Ledger afterwards — which is the point. Nothing downstream needs
//! to know a rule was involved to read a balance.
//!
//! Nothing here is domain-specific. A recurring cost, a recurring income, and a
//! recurring stock count are one shape.
//!
//! ## Occurrences are derived, never stored
//!
//! Due dates come from [`nucleus::karma::Cadence`], which is pure. Storing them
//! would duplicate a derivable fact and add a cursor to keep in sync with it.
//! Only the two things that cannot be derived are recorded:
//!
//! - **applied** — an entry exists whose `request_id` is
//!   `<recurrence_uid>:<due_at>`. `entry_revision.request_id` is already UNIQUE,
//!   so applying the same date twice is impossible without any new state, and a
//!   retried apply returns the first entry rather than moving the quantity again.
//! - **skipped** — a row in `recurrence_skip`, because "decided against" and
//!   "not looked at yet" must not read the same.

use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use nucleus::karma::Cadence;
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::exact::{decimal_columns, read_decimal};
use crate::facts::instant;

fn protocol(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.to_string())
}

pub const STATE_ACTIVE: &str = "active";
pub const STATE_PAUSED: &str = "paused";

/// The idempotency key that ties an applied entry back to the exact date of the
/// exact rule that produced it.
///
/// This string *is* the occurrence's identity. It is why no occurrence table is
/// needed: `entry_revision.request_id` is UNIQUE, so the database refuses a
/// second apply of the same date, and the read path finds applied dates by
/// looking these up.
pub fn occurrence_request_id(recurrence_uid: &str, due_at: DateTime<Utc>) -> String {
    format!("{recurrence_uid}:{}", instant(due_at))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    pub uid: String,
    pub record_uid: String,
    pub amount: DecimalValue,
    pub concept_uid: Option<String>,
    pub note: Option<String>,
    pub cadence: Cadence,
    pub anchor_at: String,
    pub state: String,
    pub revision: i64,
    pub actor_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Recurrence {
    pub fn is_paused(&self) -> bool {
        self.state == STATE_PAUSED
    }
}

/// Where one derived date stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceState {
    /// Derived, still ahead, nothing decided.
    Planned,
    /// Derived, its date has passed, and it was neither applied nor skipped.
    /// This is the one a person needs shown: an expectation nobody answered.
    Due,
    /// An entry was appended for it.
    Applied,
    /// Explicitly declined.
    Skipped,
}

impl OccurrenceState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Due => "due",
            Self::Applied => "applied",
            Self::Skipped => "skipped",
        }
    }
}

/// One derived date and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub recurrence_uid: String,
    pub due_at: DateTime<Utc>,
    pub state: OccurrenceState,
    /// The entry that applied it, when it was applied.
    pub entry_uid: Option<String>,
    /// The amount the rule declares for this date. Read from the rule as it
    /// stands now; an already-applied date reports what the entry actually
    /// carried instead, since that is what moved.
    pub amount: DecimalValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecurrenceCommit {
    Committed(Recurrence),
    Replayed(Recurrence),
}

impl RecurrenceCommit {
    pub fn rule(&self) -> &Recurrence {
        match self {
            Self::Committed(rule) | Self::Replayed(rule) => rule,
        }
    }

    pub fn was_replayed(&self) -> bool {
        matches!(self, Self::Replayed(_))
    }
}

fn map_recurrence(row: &sqlx::sqlite::SqliteRow) -> Result<Recurrence, StoreError> {
    let cadence_json: String = row.get("cadence_json");
    let cadence: Cadence = serde_json::from_str(&cadence_json)
        .map_err(|_| protocol("recurrence has an unreadable cadence"))?;
    Ok(Recurrence {
        uid: row.get("uid"),
        record_uid: row.get("record_uid"),
        amount: read_decimal(row, "amount")?,
        concept_uid: row.get("concept_uid"),
        note: row.get("note"),
        cadence,
        anchor_at: row.get("anchor_at"),
        state: row.get("state"),
        revision: row.get("revision"),
        actor_uid: row.get("actor_uid"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Recurrence>, StoreError> {
    let row = sqlx::query("SELECT * FROM recurrence WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_recurrence).transpose()
}

/// Every rule, newest first. Paused rules are included — a person managing
/// recurrence needs to see what they switched off.
pub async fn all(pool: &SqlitePool) -> Result<Vec<Recurrence>, StoreError> {
    let rows = sqlx::query("SELECT * FROM recurrence ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    rows.iter().map(map_recurrence).collect()
}

pub async fn for_record(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<Recurrence>, StoreError> {
    let rows = sqlx::query("SELECT * FROM recurrence WHERE record_uid = ? ORDER BY created_at DESC")
        .bind(record_uid)
        .fetch_all(pool)
        .await?;
    rows.iter().map(map_recurrence).collect()
}

/// The rule a request already produced, if this request has been seen.
pub async fn replayed(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<Recurrence>, StoreError> {
    let row = sqlx::query(
        "SELECT r.* FROM recurrence_revision v
           JOIN recurrence r ON r.uid = v.recurrence_uid
          WHERE v.request_id = ?",
    )
    .bind(request_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref().map(map_recurrence).transpose()
}

pub struct NewRecurrence<'a> {
    pub record_uid: &'a str,
    pub amount: DecimalValue,
    pub concept_uid: Option<&'a str>,
    pub note: Option<&'a str>,
    pub cadence: Cadence,
    pub anchor_at: DateTime<Utc>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

pub async fn create(
    pool: &SqlitePool,
    input: NewRecurrence<'_>,
    now: DateTime<Utc>,
) -> Result<RecurrenceCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("a recurring rule needs a request id"));
    }
    input
        .cadence
        .validate()
        .map_err(|error| protocol(&error.to_string()))?;
    if let Some(existing) = replayed(pool, input.request_id).await? {
        return Ok(RecurrenceCommit::Replayed(existing));
    }

    let uid = nucleus::new_uid("rec");
    let at = instant(now);
    let anchor_at = instant(input.anchor_at);
    let (mantissa, scale) = decimal_columns(input.amount);
    let cadence_json = serde_json::to_string(&input.cadence)
        .map_err(|_| protocol("cadence could not be written"))?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO recurrence
           (uid, record_uid, amount_mantissa, amount_scale, concept_uid, note,
            cadence_json, anchor_at, state, revision, actor_uid,
            created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.record_uid)
    .bind(&mantissa)
    .bind(scale)
    .bind(input.concept_uid)
    .bind(input.note)
    .bind(&cadence_json)
    .bind(&anchor_at)
    .bind(STATE_ACTIVE)
    .bind(input.actor_uid)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            recurrence_uid: &uid,
            revision: 1,
            kind: "created",
            mantissa: &mantissa,
            scale,
            concept_uid: input.concept_uid,
            note: input.note,
            cadence_json: &cadence_json,
            anchor_at: &anchor_at,
            state: STATE_ACTIVE,
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    tx.commit().await?;

    get(pool, &uid)
        .await?
        .map(RecurrenceCommit::Committed)
        .ok_or_else(|| protocol("recurring rule vanished after insert"))
}

pub struct ReviseRecurrence<'a> {
    pub recurrence_uid: &'a str,
    pub expected_revision: i64,
    pub amount: DecimalValue,
    pub concept_uid: Option<&'a str>,
    pub note: Option<&'a str>,
    pub cadence: Cadence,
    pub anchor_at: DateTime<Utc>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

/// Change what a rule expects from here on.
///
/// Dates already applied keep the amount their entry carried — those are Facts,
/// and a rule revision is not a correction of history. To fix one that was
/// applied wrongly, revise its *entry*.
pub async fn revise(
    pool: &SqlitePool,
    input: ReviseRecurrence<'_>,
    now: DateTime<Utc>,
) -> Result<RecurrenceCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("a rule revision needs a request id"));
    }
    input
        .cadence
        .validate()
        .map_err(|error| protocol(&error.to_string()))?;
    if let Some(existing) = replayed(pool, input.request_id).await? {
        return Ok(RecurrenceCommit::Replayed(existing));
    }
    let current = get(pool, input.recurrence_uid)
        .await?
        .ok_or_else(|| protocol("no such recurring rule"))?;
    if current.revision != input.expected_revision {
        return Err(protocol("recurring rule was changed by someone else"));
    }

    let revision = current.revision + 1;
    let at = instant(now);
    let anchor_at = instant(input.anchor_at);
    let (mantissa, scale) = decimal_columns(input.amount);
    let cadence_json = serde_json::to_string(&input.cadence)
        .map_err(|_| protocol("cadence could not be written"))?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE recurrence
            SET amount_mantissa = ?, amount_scale = ?, concept_uid = ?, note = ?,
                cadence_json = ?, anchor_at = ?, revision = ?,
                updated_at = ?
          WHERE uid = ? AND revision = ?",
    )
    .bind(&mantissa)
    .bind(scale)
    .bind(input.concept_uid)
    .bind(input.note)
    .bind(&cadence_json)
    .bind(&anchor_at)
    .bind(revision)
    .bind(&at)
    .bind(input.recurrence_uid)
    .bind(input.expected_revision)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            recurrence_uid: input.recurrence_uid,
            revision,
            kind: "revised",
            mantissa: &mantissa,
            scale,
            concept_uid: input.concept_uid,
            note: input.note,
            cadence_json: &cadence_json,
            anchor_at: &anchor_at,
            state: current.state.as_str(),
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    tx.commit().await?;

    get(pool, input.recurrence_uid)
        .await?
        .map(RecurrenceCommit::Committed)
        .ok_or_else(|| protocol("recurring rule vanished after revision"))
}

/// Stop or resume offering a rule's future dates.
///
/// Pausing disowns nothing: entries already applied stay, and the rule still
/// explains them.
pub async fn set_state(
    pool: &SqlitePool,
    recurrence_uid: &str,
    expected_revision: i64,
    paused: bool,
    request_id: &str,
    actor_uid: Option<&str>,
    now: DateTime<Utc>,
) -> Result<RecurrenceCommit, StoreError> {
    if request_id.trim().is_empty() {
        return Err(protocol("pausing a rule needs a request id"));
    }
    if let Some(existing) = replayed(pool, request_id).await? {
        return Ok(RecurrenceCommit::Replayed(existing));
    }
    let current = get(pool, recurrence_uid)
        .await?
        .ok_or_else(|| protocol("no such recurring rule"))?;
    if current.revision != expected_revision {
        return Err(protocol("recurring rule was changed by someone else"));
    }

    let next_state = if paused { STATE_PAUSED } else { STATE_ACTIVE };
    if current.state == next_state {
        return Ok(RecurrenceCommit::Replayed(current));
    }

    let revision = current.revision + 1;
    let at = instant(now);
    let (mantissa, scale) = decimal_columns(current.amount);
    let cadence_json = serde_json::to_string(&current.cadence)
        .map_err(|_| protocol("cadence could not be written"))?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE recurrence SET state = ?, revision = ?, updated_at = ?
          WHERE uid = ? AND revision = ?",
    )
    .bind(next_state)
    .bind(revision)
    .bind(&at)
    .bind(recurrence_uid)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            recurrence_uid,
            revision,
            kind: if paused { "paused" } else { "resumed" },
            mantissa: &mantissa,
            scale,
            concept_uid: current.concept_uid.as_deref(),
            note: current.note.as_deref(),
            cadence_json: &cadence_json,
            anchor_at: &current.anchor_at,
            state: next_state,
            request_id,
            actor_uid,
            at: &at,
        },
    )
    .await?;
    tx.commit().await?;

    get(pool, recurrence_uid)
        .await?
        .map(RecurrenceCommit::Committed)
        .ok_or_else(|| protocol("recurring rule vanished after a state change"))
}

/// Decline one date. Idempotent: skipping twice is the same decision.
pub async fn skip(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
    note: Option<&str>,
    actor_uid: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query(
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

/// Take a skip back, so the date is offered again.
pub async fn unskip(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM recurrence_skip WHERE recurrence_uid = ? AND due_at = ?")
        .bind(recurrence_uid)
        .bind(instant(due_at))
        .execute(pool)
        .await?;
    Ok(())
}

/// Derive one rule's dates in `[from, to)` and say what became of each.
///
/// A paused rule yields nothing ahead of `now`, but still reports the dates it
/// already produced — pausing is not a denial that the rule ran.
pub async fn occurrences(
    pool: &SqlitePool,
    rule: &Recurrence,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Occurrences, StoreError> {
    let anchor = parse_instant(&rule.anchor_at)?;
    // Where a rule stops is part of the rule, so the window is just the window.
    // There used to be a second end date on the row and a `min` of the two here;
    // the bound inside the cadence is now the only answer, and a one-shot is
    // simply a bound of one.
    let derived = rule
        .cadence
        .between(anchor, from, to)
        .map_err(|error| protocol(&error.to_string()))?;
    // Carried all the way to the surface. A fast rule's dates are always a
    // prefix, and a list that looks complete but is not would have a person
    // believing they had answered everything the rule expects.
    let mut result = Occurrences {
        truncated: derived.truncated,
        dates: Vec::with_capacity(derived.len()),
    };
    if derived.is_empty() {
        return Ok(result);
    }

    let skipped = skipped_dates(pool, &rule.uid).await?;
    let out = &mut result.dates;
    for due_at in derived {
        // A paused rule keeps its past but offers no future.
        if rule.is_paused() && due_at > now {
            continue;
        }
        let key = instant(due_at);
        let applied = applied_entry(pool, &rule.uid, due_at).await?;
        let (state, amount) = if let Some((_, amount)) = applied.as_ref() {
            (OccurrenceState::Applied, *amount)
        } else if skipped.contains(&key) {
            (OccurrenceState::Skipped, rule.amount)
        } else if due_at <= now {
            (OccurrenceState::Due, rule.amount)
        } else {
            (OccurrenceState::Planned, rule.amount)
        };
        out.push(Occurrence {
            recurrence_uid: rule.uid.clone(),
            due_at,
            state,
            entry_uid: applied.map(|(uid, _)| uid),
            amount,
        });
    }
    Ok(result)
}

/// One rule's derived dates, and whether the derivation ran out of room.
///
/// `truncated` is not cosmetic. A rule stepping every ten milliseconds produces
/// more dates in an hour than any surface can hold, so the only honest report is
/// "these, and more" — and a surface that cannot say so would invite someone to
/// treat a page as the whole obligation.
#[derive(Debug, Clone, Default)]
pub struct Occurrences {
    pub dates: Vec<Occurrence>,
    pub truncated: bool,
}

impl Occurrences {
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty()
    }

    pub fn len(&self) -> usize {
        self.dates.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Occurrence> {
        self.dates.iter()
    }
}

impl IntoIterator for Occurrences {
    type Item = Occurrence;
    type IntoIter = std::vec::IntoIter<Occurrence>;

    fn into_iter(self) -> Self::IntoIter {
        self.dates.into_iter()
    }
}

async fn skipped_dates(
    pool: &SqlitePool,
    recurrence_uid: &str,
) -> Result<std::collections::HashSet<String>, StoreError> {
    let rows = sqlx::query("SELECT due_at FROM recurrence_skip WHERE recurrence_uid = ?")
        .bind(recurrence_uid)
        .fetch_all(pool)
        .await?;
    Ok(rows.iter().map(|row| row.get::<String, _>("due_at")).collect())
}

/// The entry that applied one date, found by the request id that names it.
async fn applied_entry(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
) -> Result<Option<(String, DecimalValue)>, StoreError> {
    let row = sqlx::query(
        "SELECT e.uid AS uid, e.amount_mantissa AS amount_mantissa,
                e.amount_scale AS amount_scale
           FROM entry_revision v
           JOIN entry e ON e.uid = v.entry_uid
          WHERE v.request_id = ?",
    )
    .bind(occurrence_request_id(recurrence_uid, due_at))
    .fetch_optional(pool)
    .await?;
    match row {
        Some(row) => Ok(Some((row.get("uid"), read_decimal(&row, "amount")?))),
        None => Ok(None),
    }
}

fn parse_instant(text: &str) -> Result<DateTime<Utc>, StoreError> {
    DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| protocol("recurring rule holds an unreadable timestamp"))
}

struct RevisionRow<'a> {
    recurrence_uid: &'a str,
    revision: i64,
    kind: &'a str,
    mantissa: &'a str,
    scale: i64,
    concept_uid: Option<&'a str>,
    note: Option<&'a str>,
    cadence_json: &'a str,
    anchor_at: &'a str,
    state: &'a str,
    request_id: &'a str,
    actor_uid: Option<&'a str>,
    at: &'a str,
}

async fn insert_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    row: RevisionRow<'_>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO recurrence_revision
           (uid, recurrence_uid, revision, kind, amount_mantissa, amount_scale,
            concept_uid, note, cadence_json, anchor_at, state,
            request_id, actor_uid, at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("recr"))
    .bind(row.recurrence_uid)
    .bind(row.revision)
    .bind(row.kind)
    .bind(row.mantissa)
    .bind(row.scale)
    .bind(row.concept_uid)
    .bind(row.note)
    .bind(row.cadence_json)
    .bind(row.anchor_at)
    .bind(row.state)
    .bind(row.request_id)
    .bind(row.actor_uid)
    .bind(row.at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
