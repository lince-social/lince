use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use nucleus::karma::{Cadence, Carry, Consequences, Gate};
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use crate::exact::read_decimal;
use crate::facts::instant;

fn protocol(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.to_string())
}

pub const STATE_ACTIVE: &str = "active";
pub const STATE_PAUSED: &str = "paused";

pub fn occurrence_request_id(recurrence_uid: &str, due_at: DateTime<Utc>) -> String {
    format!("{recurrence_uid}:{}", instant(due_at))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    pub uid: String,
    pub record_uid: String,
    pub consequences: Consequences,
    pub condition: Option<RuleCondition>,
    pub note: Option<String>,
    pub cadence: Cadence,
    pub anchor_at: String,
    pub state: String,
    pub revision: i64,
    pub actor_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleCondition {
    pub source: String,
    pub gate: Gate,
    pub carry: Carry,
}

impl Recurrence {
    pub fn is_paused(&self) -> bool {
        self.state == STATE_PAUSED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceState {
    Planned,
    Due,
    Applied,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub recurrence_uid: String,
    pub due_at: DateTime<Utc>,
    pub state: OccurrenceState,
    pub entry_uid: Option<String>,
    pub amount: Option<DecimalValue>,
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

fn read_condition(row: &sqlx::sqlite::SqliteRow) -> Result<Option<RuleCondition>, StoreError> {
    let Some(source) = row.get::<Option<String>, _>("condition_src") else {
        return Ok(None);
    };
    let gate = row
        .get::<Option<String>, _>("gate")
        .ok_or_else(|| protocol("rule has a condition but no gate"))?;
    let carry = row
        .get::<Option<String>, _>("carry")
        .ok_or_else(|| protocol("rule has a condition but no carry"))?;
    Ok(Some(RuleCondition {
        source,
        gate: Gate::parse(&gate).map_err(|_| protocol("rule has an unreadable gate"))?,
        carry: Carry::parse(&carry).map_err(|_| protocol("rule has an unreadable carry"))?,
    }))
}

fn condition_columns(
    condition: Option<&RuleCondition>,
) -> (Option<String>, Option<String>, Option<String>) {
    match condition {
        Some(c) => (
            Some(c.source.clone()),
            Some(c.gate.as_text()),
            Some(c.carry.as_text()),
        ),
        None => (None, None, None),
    }
}

fn map_recurrence(row: &sqlx::sqlite::SqliteRow) -> Result<Recurrence, StoreError> {
    let cadence_json: String = row.get("cadence_json");
    let cadence: Cadence = serde_json::from_str(&cadence_json)
        .map_err(|_| protocol("recurrence has an unreadable cadence"))?;
    let consequences_json: String = row.get("consequences_json");
    let consequences: Consequences = serde_json::from_str(&consequences_json)
        .map_err(|_| protocol("recurrence has unreadable consequences"))?;
    Ok(Recurrence {
        uid: row.get("uid"),
        record_uid: row.get("record_uid"),
        consequences,
        condition: read_condition(row)?,
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
    let rows =
        sqlx::query("SELECT * FROM recurrence WHERE record_uid = ? ORDER BY created_at DESC")
            .bind(record_uid)
            .fetch_all(pool)
            .await?;
    rows.iter().map(map_recurrence).collect()
}

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
    pub consequences: Consequences,
    pub condition: Option<RuleCondition>,
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
    let cadence_json = serde_json::to_string(&input.cadence)
        .map_err(|_| protocol("cadence could not be written"))?;
    let consequences_json = serde_json::to_string(&input.consequences)
        .map_err(|_| protocol("consequences could not be written"))?;
    let (condition_src, gate, carry) = condition_columns(input.condition.as_ref());

    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
        "INSERT INTO recurrence
           (uid, record_uid, consequences_json, condition_src, gate, carry, note,
            cadence_json, anchor_at, state, revision, actor_uid,
            created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.record_uid)
    .bind(&consequences_json)
    .bind(&condition_src)
    .bind(&gate)
    .bind(&carry)
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
            consequences_json: &consequences_json,
            condition_src: condition_src.as_deref(),
            gate: gate.as_deref(),
            carry: carry.as_deref(),
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
    pub consequences: Consequences,
    pub condition: Option<RuleCondition>,
    pub note: Option<&'a str>,
    pub cadence: Cadence,
    pub anchor_at: DateTime<Utc>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

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
    let cadence_json = serde_json::to_string(&input.cadence)
        .map_err(|_| protocol("cadence could not be written"))?;
    let consequences_json = serde_json::to_string(&input.consequences)
        .map_err(|_| protocol("consequences could not be written"))?;
    let (condition_src, gate, carry) = condition_columns(input.condition.as_ref());

    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
        "UPDATE recurrence
            SET consequences_json = ?, note = ?,
                cadence_json = ?, anchor_at = ?, revision = ?,
                updated_at = ?
          WHERE uid = ? AND revision = ?",
    )
    .bind(&consequences_json)
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
            consequences_json: &consequences_json,
            condition_src: condition_src.as_deref(),
            gate: gate.as_deref(),
            carry: carry.as_deref(),
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
    let cadence_json = serde_json::to_string(&current.cadence)
        .map_err(|_| protocol("cadence could not be written"))?;
    let consequences_json = serde_json::to_string(&current.consequences)
        .map_err(|_| protocol("consequences could not be written"))?;
    let (condition_src, gate, carry) = condition_columns(current.condition.as_ref());

    let mut tx = crate::write_tx(pool).await?;
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
            consequences_json: &consequences_json,
            condition_src: condition_src.as_deref(),
            gate: gate.as_deref(),
            carry: carry.as_deref(),
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

pub async fn occurrences(
    pool: &SqlitePool,
    rule: &Recurrence,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Occurrences, StoreError> {
    let anchor = parse_instant(&rule.anchor_at)?;
    let derived = rule
        .cadence
        .between(anchor, from, to)
        .map_err(|error| protocol(&error.to_string()))?;
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
        if rule.is_paused() && due_at > now {
            continue;
        }
        let key = instant(due_at);
        let applied = applied_entry(pool, &rule.uid, due_at).await?;
        let declared = rule.consequences.declared_delta().copied();
        let (state, amount) = if let Some((_, amount)) = applied.as_ref() {
            (OccurrenceState::Applied, Some(*amount))
        } else if skipped.contains(&key) {
            (OccurrenceState::Skipped, declared)
        } else if due_at <= now {
            (OccurrenceState::Due, declared)
        } else {
            (OccurrenceState::Planned, declared)
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
    Ok(rows
        .iter()
        .map(|row| row.get::<String, _>("due_at"))
        .collect())
}

pub async fn applied(
    pool: &SqlitePool,
    recurrence_uid: &str,
    due_at: DateTime<Utc>,
) -> Result<Option<String>, StoreError> {
    Ok(applied_entry(pool, recurrence_uid, due_at)
        .await?
        .map(|(uid, _)| uid))
}

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
    consequences_json: &'a str,
    condition_src: Option<&'a str>,
    gate: Option<&'a str>,
    carry: Option<&'a str>,
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
           (uid, recurrence_uid, revision, kind, consequences_json,
            condition_src, gate, carry,
            note, cadence_json, anchor_at, state,
            request_id, actor_uid, at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("recr"))
    .bind(row.recurrence_uid)
    .bind(row.revision)
    .bind(row.kind)
    .bind(row.consequences_json)
    .bind(row.condition_src)
    .bind(row.gate)
    .bind(row.carry)
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

pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("DELETE FROM recurrence_skip WHERE recurrence_uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM recurrence_revision WHERE recurrence_uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    let gone = sqlx::query("DELETE FROM recurrence WHERE uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(gone > 0)
}
