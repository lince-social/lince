//! Received outcomes used by Transfer dependencies.

use chrono::{DateTime, FixedOffset, Utc};
use nucleus::transfer_delivery::TransferEnvelopeV1;
use serde_json::{Value, from_str, json};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::StoreError;

/// The maximum accepted age of an upstream outcome.
pub const MAX_REMOTE_AGE_SECONDS: i64 = 120;
const MAX_FUTURE_SKEW_SECONDS: i64 = 30;

/// Checks the origin timestamp against the freshness window.
pub fn fresh(created: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(created).is_ok_and(|created| fresh_at(created, now))
}

fn fresh_at(created: DateTime<FixedOffset>, now: DateTime<Utc>) -> bool {
    (-MAX_FUTURE_SKEW_SECONDS..=MAX_REMOTE_AGE_SECONDS)
        .contains(&now.signed_duration_since(created).num_seconds())
}

/// The disclosed agreement and settlement status, or an unavailability reason.
#[derive(Debug)]
pub struct Outcome {
    pub agreed: bool,
    pub settled: bool,
    pub evidence: Value,
    pub unavailable: Option<&'static str>,
}

impl Outcome {
    /// Represents an outcome that cannot be established from current evidence.
    pub fn unavailable(reason: &'static str) -> Self {
        Self {
            agreed: false,
            settled: false,
            evidence: Value::Null,
            unavailable: Some(reason),
        }
    }
}

pub(crate) async fn remember_on(
    connection: &mut SqliteConnection,
    reference: &str,
    envelope: &TransferEnvelopeV1,
    payload: &str,
    received_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_outcome_evidence
         (envelope_uid, reference_uid, cursor, revision, policy_revision,
          origin_created_at, received_at, payload, payload_hash)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&envelope.envelope_uid)
    .bind(reference)
    .bind(envelope.cursor.cast_signed())
    .bind(envelope.transfer_revision.cast_signed())
    .bind(envelope.delivery_policy_revision.cast_signed())
    .bind(&envelope.created_at)
    .bind(received_at)
    .bind(payload)
    .bind(&envelope.payload_hash)
    .execute(connection)
    .await?;
    Ok(())
}

pub(crate) async fn source_on(
    connection: &mut SqliteConnection,
    downstream: &str,
    upstream: &str,
) -> Result<(Option<String>, Option<String>), StoreError> {
    let local = sqlx::query_scalar::<_, String>(
        "SELECT r.organ_uid FROM transfer t
         JOIN record r ON r.uid = t.record_uid
         JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ'
             AND own.kind = 'organ' AND own.deleted_at IS NULL
         WHERE t.record_uid = ? AND r.deleted_at IS NULL",
    )
    .bind(upstream)
    .fetch_optional(&mut *connection)
    .await?;
    if let Some(origin) = local {
        return Ok((Some(origin), None));
    }
    let rows = sqlx::query(
        "SELECT r.uid, r.origin_organ_uid FROM transfer_remote_reference r
         WHERE r.transfer_uid = ? AND r.state = 'active' AND r.projection IS NOT NULL
           AND r.recipient_organ_uid = (
               SELECT uid FROM record WHERE slug = 'local-organ'
                 AND kind = 'organ' AND deleted_at IS NULL
           )
           AND r.recipient_person_uid IN (SELECT actor_uid FROM transfer_party WHERE transfer_uid = ?)
         ORDER BY r.last_transfer_revision DESC, r.last_cursor DESC, r.uid",
    )
    .bind(upstream)
    .bind(downstream)
    .fetch_all(connection)
    .await?;
    if let Some((first, rest)) = rows.split_first() {
        let origin: &str = first.get("origin_organ_uid");
        if rest
            .iter()
            .all(|row| row.get::<&str, _>("origin_organ_uid") == origin)
        {
            return Ok((Some(origin.into()), Some(first.get("uid"))));
        }
    }
    Err(StoreError::Protocol(
        "upstream outcome needs one authorized origin and a participant's received evidence".into(),
    ))
}

/// Evaluates a dependency using its latest authorized and fresh received outcome.
///
/// # Errors
///
/// Returns an error if the query fails or the saved envelope is malformed.
pub async fn remote_on(
    connection: &mut SqliteConnection,
    dependency: &str,
    now: DateTime<Utc>,
) -> Result<Outcome, StoreError> {
    let row = sqlx::query(
        "SELECT r.state, r.policy_revision, r.last_transfer_revision, e.payload,
                e.origin_created_at, e.revision, e.policy_revision AS evidence_policy_revision
         FROM transfer_dependency d
         JOIN transfer_remote_reference r ON r.uid = d.upstream_reference_uid
             AND r.origin_organ_uid = d.upstream_origin_uid AND r.transfer_uid = d.upstream_uid
         JOIN record own ON own.uid = r.recipient_organ_uid AND own.slug = 'local-organ'
             AND own.kind = 'organ' AND own.deleted_at IS NULL
         LEFT JOIN transfer_outcome_evidence e ON e.envelope_uid = r.last_envelope_uid
             AND e.reference_uid = r.uid AND e.cursor = r.last_cursor
         WHERE d.uid = ? AND EXISTS(
             SELECT 1 FROM transfer_party p
             WHERE p.transfer_uid = d.transfer_uid AND p.actor_uid = r.recipient_person_uid
         )",
    )
    .bind(dependency)
    .fetch_optional(connection)
    .await?;
    let Some(row) = row else {
        return Ok(Outcome::unavailable("upstream_evidence_unavailable"));
    };
    if row.get::<&str, _>("state") != "active" {
        return Ok(Outcome::unavailable("upstream_access_revoked"));
    }
    let Some(payload) = row.get::<Option<&str>, _>("payload") else {
        return Ok(Outcome::unavailable("upstream_evidence_unavailable"));
    };
    if row.get::<i64, _>("revision") != row.get::<i64, _>("last_transfer_revision")
        || row.get::<i64, _>("evidence_policy_revision") != row.get::<i64, _>("policy_revision")
    {
        return Ok(Outcome::unavailable("upstream_evidence_revision_changed"));
    }
    let Ok(created) = DateTime::parse_from_rfc3339(row.get("origin_created_at")) else {
        return Ok(Outcome::unavailable("upstream_evidence_time_invalid"));
    };
    if !fresh_at(created, now) {
        return Ok(Outcome::unavailable("upstream_evidence_stale"));
    }
    let envelope: TransferEnvelopeV1 =
        from_str(payload).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let summary = envelope.projection.get("outcome").unwrap_or(&Value::Null);
    let (Some(agreed), Some(settled)) = (
        summary.get("agreed").and_then(Value::as_bool),
        summary.get("settled").and_then(Value::as_bool),
    ) else {
        return Ok(Outcome::unavailable("upstream_outcome_not_disclosed"));
    };
    if summary.get("revision").and_then(Value::as_u64) != Some(envelope.transfer_revision) {
        return Ok(Outcome::unavailable("upstream_evidence_revision_changed"));
    }
    Ok(Outcome {
        agreed,
        settled,
        unavailable: None,
        evidence: json!({"kind":"received_outcome", "envelope":envelope.envelope_uid,
        "origin":envelope.origin_organ_uid,"transfer":envelope.transfer_uid,"revision":envelope.transfer_revision,
        "cursor":envelope.cursor,"hash":envelope.payload_hash,"at":envelope.created_at,"fresh_for_seconds":MAX_REMOTE_AGE_SECONDS}),
    })
}

/// Lists distinct upstream kinds and UIDs from the signed revision history.
///
/// # Errors
///
/// Returns an error if the history query or decoding fails.
pub async fn history_upstreams(
    pool: &SqlitePool,
    transfer: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    sqlx::query_as(
        "SELECT DISTINCT json_extract(dependency.value, '$.upstream_kind'),
                         json_extract(dependency.value, '$.upstream_uid')
         FROM transfer_revision r JOIN fact f ON f.uid = r.fact_uid,
              json_each(f.payload, '$.terms.dependencies') dependency
         WHERE r.transfer_uid = ? ORDER BY 1, 2",
    )
    .bind(transfer)
    .fetch_all(pool)
    .await
}
