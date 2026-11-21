use nucleus::karma::{Cadence, Carry, Consequences, Gate};
use nucleus::performance::{Span, add};
use serde_json::from_str;
use sqlx::{Row, SqlitePool, query, sqlite::SqliteRow};

use super::{RuleCondition, STATE_PAUSED, protocol};
use crate::StoreError;

/// A stored recurring rule and its latest revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recurrence {
    /// Stable rule identity.
    pub uid: String,
    /// Record on which the rule acts.
    pub record_uid: String,
    /// Work performed by each applicable occurrence.
    pub consequences: Consequences,
    /// Optional guards and reference bindings.
    pub condition: Option<RuleCondition>,
    /// Optional note supplied by the author.
    pub note: Option<String>,
    /// Date derivation policy for the rule.
    pub cadence: Cadence,
    /// Millisecond-aligned start timestamp in stored UTC form.
    pub anchor_at: String,
    /// Stored active or paused state.
    pub state: String,
    /// Revision used to reject edits based on stale data.
    pub revision: i64,
    /// Current attributed actor, when known.
    pub actor_uid: Option<String>,
    /// Creation timestamp in stored UTC form.
    pub created_at: String,
    /// Latest update timestamp in stored UTC form.
    pub updated_at: String,
}

impl Recurrence {
    /// Returns whether future occurrences are suppressed by a pause.
    pub fn is_paused(&self) -> bool {
        self.state == STATE_PAUSED
    }
}

fn read_condition(row: &SqliteRow) -> Result<Option<RuleCondition>, StoreError> {
    let Some(source) = row.try_get::<Option<String>, _>("condition_src")? else {
        return Ok(None);
    };
    let gate = row
        .try_get::<Option<&str>, _>("gate")?
        .ok_or_else(|| protocol("rule has a condition but no gate"))?;
    let carry = row
        .try_get::<Option<&str>, _>("carry")?
        .ok_or_else(|| protocol("rule has a condition but no carry"))?;
    Ok(Some(RuleCondition::new(
        source,
        from_str(row.try_get("bindings_json")?).map_err(|error| protocol(error.to_string()))?,
        Gate::parse(gate)
            .map_err(|error| protocol(format!("rule has an unreadable gate: {error}")))?,
        Carry::parse(carry)
            .map_err(|error| protocol(format!("rule has an unreadable carry: {error}")))?,
    )))
}

fn map_recurrence(row: SqliteRow) -> Result<Recurrence, StoreError> {
    let _decode = Span::new("karma-rule-row-decode");
    add("karma-rule-row-decodes", 1);
    let cadence = from_str(row.try_get("cadence_json")?)
        .map_err(|error| protocol(format!("recurrence has an unreadable cadence: {error}")))?;
    let consequences = from_str(row.try_get("consequences_json")?)
        .map_err(|error| protocol(format!("recurrence has unreadable consequences: {error}")))?;
    Ok(Recurrence {
        uid: row.try_get("uid")?,
        record_uid: row.try_get("record_uid")?,
        consequences,
        condition: read_condition(&row)?,
        note: row.try_get("note")?,
        cadence,
        anchor_at: row.try_get("anchor_at")?,
        state: row.try_get("state")?,
        revision: row.try_get("revision")?,
        actor_uid: row.try_get("actor_uid")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Reads a recurring rule by identity if it exists.
///
/// # Errors
/// Returns an error if the query or rule decoding fails.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Recurrence>, StoreError> {
    query("SELECT * FROM recurrence WHERE uid = ?")
        .bind(uid)
        .try_map(map_recurrence)
        .fetch_optional(pool)
        .await
}

/// Lists rules on live records, newest first.
///
/// # Errors
/// Returns an error if the query or any rule decoding fails.
pub async fn all(pool: &SqlitePool) -> Result<Vec<Recurrence>, StoreError> {
    query("SELECT recurrence.* FROM recurrence JOIN record ON record.uid=recurrence.record_uid WHERE record.deleted_at IS NULL ORDER BY recurrence.created_at DESC")
        .try_map(map_recurrence)
        .fetch_all(pool)
        .await
}

/// Lists rules on the given record, newest first.
///
/// # Errors
/// Returns an error if the query or any rule decoding fails.
pub async fn for_record(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<Recurrence>, StoreError> {
    query("SELECT * FROM recurrence WHERE record_uid = ? ORDER BY created_at DESC")
        .bind(record_uid)
        .try_map(map_recurrence)
        .fetch_all(pool)
        .await
}

/// Reads the current rule associated with a previously recorded mutation request.
///
/// # Errors
/// Returns an error if the query or rule decoding fails.
pub async fn replayed(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<Recurrence>, StoreError> {
    query(
        "SELECT r.* FROM recurrence_revision v
           JOIN recurrence r ON r.uid = v.recurrence_uid
          WHERE v.request_id = ?",
    )
    .bind(request_id)
    .try_map(map_recurrence)
    .fetch_optional(pool)
    .await
}
