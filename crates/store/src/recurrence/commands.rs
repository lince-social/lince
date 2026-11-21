use chrono::{DateTime, Utc};
use nucleus::{
    execution::now,
    karma::{Cadence, Consequences},
    new_uid,
    performance::{Span, add},
};
use serde_json::to_string;
use sqlx::{Sqlite, SqlitePool, Transaction, query};

use super::{Recurrence, RuleCondition, STATE_ACTIVE, STATE_PAUSED, get, protocol, replayed};
use crate::{
    StoreError, facts::instant, karma_bindings, karma_commands, karma_fields, karma_schedules,
    karma_stages, write_tx,
};

/// The rule returned by a newly committed or previously recorded request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecurrenceCommit {
    /// This request wrote a new rule state.
    Committed(Recurrence),
    /// This request had already been applied, or the requested state already held.
    Replayed(Recurrence),
}

impl RecurrenceCommit {
    /// Borrows the current rule returned by the request.
    pub fn rule(&self) -> &Recurrence {
        match self {
            Self::Committed(rule) | Self::Replayed(rule) => rule,
        }
    }

    /// Reports whether the request returned an existing state.
    pub fn was_replayed(&self) -> bool {
        matches!(self, Self::Replayed(_))
    }
}

struct EncodedRule<'a> {
    cadence: String,
    consequences: String,
    condition_src: Option<&'a str>,
    gate: Option<String>,
    carry: Option<String>,
}

impl<'a> EncodedRule<'a> {
    fn new(
        cadence: &Cadence,
        consequences: &Consequences,
        condition: Option<&'a RuleCondition>,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            cadence: to_string(cadence)
                .map_err(|error| protocol(format!("cadence could not be written: {error}")))?,
            consequences: to_string(consequences)
                .map_err(|error| protocol(format!("consequences could not be written: {error}")))?,
            condition_src: condition.map(|value| value.source.as_str()),
            gate: condition.map(|value| value.gate.as_text()),
            carry: condition.map(|value| value.carry.as_text()),
        })
    }
}

fn stored_anchor(anchor_at: DateTime<Utc>) -> Result<String, StoreError> {
    DateTime::from_timestamp_millis(anchor_at.timestamp_millis())
        .map(instant)
        .ok_or_else(|| protocol("rule anchor is out of range"))
}

fn next_revision(current: &Recurrence) -> Result<i64, StoreError> {
    current
        .revision
        .checked_add(1)
        .ok_or_else(|| protocol("rule revision is out of range"))
}

/// Inputs for creating a recurring rule.
#[expect(
    missing_debug_implementations,
    reason = "Creation inputs may contain private rule source and notes"
)]
pub struct NewRecurrence<'a> {
    /// Record on which the rule acts.
    pub record_uid: &'a str,
    /// Work the rule performs.
    pub consequences: Consequences,
    /// Optional guard and reference bindings.
    pub condition: Option<RuleCondition>,
    /// Author's optional note.
    pub note: Option<&'a str>,
    /// Policy for deriving occurrence dates.
    pub cadence: Cadence,
    /// Start timestamp, stored at millisecond precision.
    pub anchor_at: DateTime<Utc>,
    /// Stable request identity used for replays.
    pub request_id: &'a str,
    /// Actor to attribute the creation to, when known.
    pub actor_uid: Option<&'a str>,
}

/// Creates a recurring rule with a generated identity or replays the request.
///
/// # Errors
/// Returns an error for invalid input or failed storage operations.
pub async fn create(
    pool: &SqlitePool,
    input: NewRecurrence<'_>,
    now: DateTime<Utc>,
) -> Result<RecurrenceCommit, StoreError> {
    create_identified(pool, input, None, now).await
}

/// Creates or imports a recurring rule with an optional caller-supplied identity.
///
/// # Errors
/// Returns an error for invalid input, a conflicting replay, or failed storage operations.
pub async fn create_identified(
    pool: &SqlitePool,
    input: NewRecurrence<'_>,
    uid: Option<&str>,
    now: DateTime<Utc>,
) -> Result<RecurrenceCommit, StoreError> {
    if input.request_id.trim().is_empty() {
        return Err(protocol("a recurring rule needs a request id"));
    }
    input
        .cadence
        .validate()
        .map_err(|error| protocol(error.to_string()))?;
    if let Some(existing) = replayed(pool, input.request_id).await? {
        if uid.is_some_and(|uid| uid != existing.uid) {
            return Err(protocol("rule import request belongs to another uid"));
        }
        return Ok(RecurrenceCommit::Replayed(existing));
    }

    let uid = uid.map_or_else(|| new_uid("rec"), str::to_owned);
    let at = instant(now);
    let anchor_at = stored_anchor(input.anchor_at)?;
    let content = EncodedRule::new(
        &input.cadence,
        &input.consequences,
        input.condition.as_ref(),
    )?;

    let mut tx = write_tx(pool).await?;
    query(
        "INSERT INTO recurrence
           (uid, record_uid, consequences_json, condition_src, gate, carry, note,
            cadence_json, anchor_at, state, revision, actor_uid,
            created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(input.record_uid)
    .bind(&content.consequences)
    .bind(content.condition_src)
    .bind(&content.gate)
    .bind(&content.carry)
    .bind(input.note)
    .bind(&content.cadence)
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
            content: &content,
            note: input.note,
            anchor_at: &anchor_at,
            state: STATE_ACTIVE,
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    karma_fields::replace_inline(&mut tx, &uid).await?;
    karma_bindings::save(
        &mut tx,
        &uid,
        content.condition_src,
        input
            .condition
            .as_ref()
            .map_or(&[], |condition| condition.bindings.as_slice()),
    )
    .await?;
    tx.commit().await?;

    get(pool, &uid)
        .await?
        .map(RecurrenceCommit::Committed)
        .ok_or_else(|| protocol("recurring rule vanished after insert"))
}

/// Replacement definition and revision guard for an existing rule.
#[expect(
    missing_debug_implementations,
    reason = "Revision inputs may contain private rule source and notes"
)]
pub struct ReviseRecurrence<'a> {
    /// Existing rule identity.
    pub recurrence_uid: &'a str,
    /// Revision that must still be current.
    pub expected_revision: i64,
    /// Work the revised rule performs.
    pub consequences: Consequences,
    /// Optional guard for the revised rule.
    pub condition: Option<RuleCondition>,
    /// Author's replacement note, if supplied.
    pub note: Option<&'a str>,
    /// Replacement date derivation policy.
    pub cadence: Cadence,
    /// Replacement anchor, stored at millisecond precision.
    pub anchor_at: DateTime<Utc>,
    /// Stable request identity used for replays.
    pub request_id: &'a str,
    /// Actor to attribute the revision to, when known.
    pub actor_uid: Option<&'a str>,
}

/// Replaces a rule's definition and reflects the edit in its schedules.
///
/// # Errors
/// Returns an error for invalid input, a stale revision, or failed storage operations.
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
        .map_err(|error| protocol(error.to_string()))?;
    if let Some(existing) = replayed(pool, input.request_id).await? {
        return Ok(RecurrenceCommit::Replayed(existing));
    }
    let current = get(pool, input.recurrence_uid)
        .await?
        .ok_or_else(|| protocol("no such recurring rule"))?;
    if current.revision != input.expected_revision {
        return Err(protocol("recurring rule was changed by someone else"));
    }

    let revision = next_revision(&current)?;
    let at = instant(now);
    let anchor_at = stored_anchor(input.anchor_at)?;
    let content = EncodedRule::new(
        &input.cadence,
        &input.consequences,
        input.condition.as_ref(),
    )?;

    let mut tx = write_tx(pool).await?;
    query(
        "UPDATE recurrence
            SET consequences_json = ?, note = ?,
                cadence_json = ?, anchor_at = ?, revision = ?,
                updated_at = ?, condition_src = ?, gate = ?, carry = ?, actor_uid = ?
          WHERE uid = ? AND revision = ?",
    )
    .bind(&content.consequences)
    .bind(input.note)
    .bind(&content.cadence)
    .bind(&anchor_at)
    .bind(revision)
    .bind(&at)
    .bind(content.condition_src)
    .bind(&content.gate)
    .bind(&content.carry)
    .bind(input.actor_uid)
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
            content: &content,
            note: input.note,
            anchor_at: &anchor_at,
            state: current.state.as_str(),
            request_id: input.request_id,
            actor_uid: input.actor_uid,
            at: &at,
        },
    )
    .await?;
    karma_fields::replace_inline(&mut tx, input.recurrence_uid).await?;
    karma_bindings::save(
        &mut tx,
        input.recurrence_uid,
        content.condition_src,
        current
            .condition
            .as_ref()
            .map_or(&[], |condition| condition.bindings.as_slice()),
    )
    .await?;
    karma_schedules::reflect_rule_edit(&mut tx, input.recurrence_uid, now).await?;
    tx.commit().await?;

    get(pool, input.recurrence_uid)
        .await?
        .map(RecurrenceCommit::Committed)
        .ok_or_else(|| protocol("recurring rule vanished after revision"))
}

/// Pauses or resumes a rule, recording the transition in its revision history.
///
/// # Errors
/// Returns an error for an empty request, missing rule, stale revision, or failed write.
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

    let (next_state, kind) = if paused {
        (STATE_PAUSED, "paused")
    } else {
        (STATE_ACTIVE, "resumed")
    };
    if current.state == next_state {
        return Ok(RecurrenceCommit::Replayed(current));
    }

    let revision = next_revision(&current)?;
    let at = instant(now);
    let content = EncodedRule::new(
        &current.cadence,
        &current.consequences,
        current.condition.as_ref(),
    )?;

    let mut tx = write_tx(pool).await?;
    query(
        "UPDATE recurrence SET state = ?, revision = ?, updated_at = ?, actor_uid = CASE WHEN ? THEN actor_uid ELSE ? END
          WHERE uid = ? AND revision = ?",
    )
    .bind(next_state)
    .bind(revision)
    .bind(&at)
    .bind(paused)
    .bind(actor_uid)
    .bind(recurrence_uid)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    insert_revision(
        &mut tx,
        RevisionRow {
            recurrence_uid,
            revision,
            kind,
            content: &content,
            note: current.note.as_deref(),
            anchor_at: &current.anchor_at,
            state: next_state,
            request_id,
            actor_uid,
            at: &at,
        },
    )
    .await?;
    karma_schedules::reflect_rule_edit(&mut tx, recurrence_uid, now).await?;
    tx.commit().await?;

    get(pool, recurrence_uid)
        .await?
        .map(RecurrenceCommit::Committed)
        .ok_or_else(|| protocol("recurring rule vanished after a state change"))
}

struct RevisionRow<'a> {
    recurrence_uid: &'a str,
    revision: i64,
    kind: &'a str,
    content: &'a EncodedRule<'a>,
    note: Option<&'a str>,
    anchor_at: &'a str,
    state: &'a str,
    request_id: &'a str,
    actor_uid: Option<&'a str>,
    at: &'a str,
}

async fn insert_revision(
    tx: &mut Transaction<'_, Sqlite>,
    row: RevisionRow<'_>,
) -> Result<(), StoreError> {
    let _insert = Span::new("karma-rule-revision-insert-await");
    query(
        "INSERT INTO recurrence_revision
           (uid, recurrence_uid, revision, kind, consequences_json,
            condition_src, gate, carry,
            note, cadence_json, anchor_at, state,
            request_id, actor_uid, at, bindings_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 (SELECT bindings_json FROM recurrence WHERE uid = ?))",
    )
    .bind(new_uid("recr"))
    .bind(row.recurrence_uid)
    .bind(row.revision)
    .bind(row.kind)
    .bind(&row.content.consequences)
    .bind(row.content.condition_src)
    .bind(&row.content.gate)
    .bind(&row.content.carry)
    .bind(row.note)
    .bind(&row.content.cadence)
    .bind(row.anchor_at)
    .bind(row.state)
    .bind(row.request_id)
    .bind(row.actor_uid)
    .bind(row.at)
    .bind(row.recurrence_uid)
    .execute(&mut **tx)
    .await?;
    add("karma-rule-revision-inserts", 1);
    Ok(())
}

/// Deletes a rule and invalidates its dependent work, returning whether it existed.
///
/// # Errors
/// Returns an error if dependent updates or deletion fail.
pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let mut tx = write_tx(pool).await?;
    karma_stages::invalidate_children_tx(&mut tx, uid, now()).await?;
    karma_commands::invalidate_children_tx(&mut tx, uid).await?;
    query("DELETE FROM recurrence_skip WHERE recurrence_uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM recurrence_revision WHERE recurrence_uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    let gone = query("DELETE FROM recurrence WHERE uid = ?")
        .bind(uid)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(gone > 0)
}
