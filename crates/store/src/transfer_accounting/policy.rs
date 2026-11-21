//! Signed policy history and the current policy for a Transfer, exchange, and Person.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use nucleus::{
    fact::sha256_hex,
    new_uid,
    transfer::application::{EffectMode, PrivateEffect, validate},
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Row, Sqlite, SqliteConnection, SqlitePool, sqlite::SqliteRow};

use super::invalid;
use crate::{
    StoreError, transfer_effects, transfer_replication::require_private_writer,
    transfer_stock::validate_all_on, write_tx,
};

/// The saved, signed policy revision for a Person's private application.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    /// Identifier of the immutable policy event.
    pub uid: String,
    /// Transfer to which the policy applies.
    pub transfer: String,
    /// Exchange within the Transfer.
    pub exchange: String,
    /// Person whose local Records receive the effects.
    pub person: String,
    /// Destination of the primary effect.
    pub record: String,
    /// Formula for the primary effect.
    pub formula: String,
    /// Effects in application order, starting with the primary effect.
    pub effects: Vec<PrivateEffect>,
    /// Revision number within the Transfer, exchange, and Person.
    pub version: u64,
    /// Hash of the normalized payload signed by the owner.
    pub hash: String,
}

/// A requested replacement for the current private application policy.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    /// Transfer to which the policy applies.
    pub transfer: String,
    /// Exchange within the Transfer.
    pub exchange: String,
    /// Person submitting the policy for their local Records.
    pub person: String,
    /// Ordered effects targeting distinct live Records in the local Organ.
    pub effects: Vec<PrivateEffect>,
    /// Reviewed revision, or zero for the first policy.
    pub expected_version: u64,
    /// Idempotency key for this request and its normalized payload.
    pub request_id: String,
}

fn map_policy(row: SqliteRow) -> Result<Policy, StoreError> {
    let mut effects: Vec<PrivateEffect> =
        serde_json::from_str(row.get::<&str, _>("effects_json")).map_err(invalid)?;
    if effects.is_empty() {
        effects.push(PrivateEffect {
            record: row.get("record_uid"),
            formula: row.get("formula"),
            mode: EffectMode::Quantity,
        });
    }
    Ok(Policy {
        uid: row.get("uid"),
        transfer: row.get("transfer_uid"),
        exchange: row.get("exchange_uid"),
        person: row.get("person_uid"),
        record: row.get("record_uid"),
        formula: row.get("formula"),
        effects,
        version: row.get::<i64, _>("version").cast_unsigned(),
        hash: row.get("policy_hash"),
    })
}

/// Read the current policy, if one has been set.
///
/// # Errors
///
/// Returns an error if the query fails or the saved effects cannot be decoded.
pub async fn policy(
    pool: &SqlitePool,
    transfer: &str,
    exchange: &str,
    person: &str,
) -> Result<Option<Policy>, StoreError> {
    policy_on(&mut *pool.acquire().await?, transfer, exchange, person).await
}

/// Save a new signed revision or return the original event on exact replay.
///
/// # Errors
///
/// Returns an error for invalid effects, unauthorized writers or destinations,
/// a stale revision, a conflicting request ID, a missing signature, or a
/// failed write or stock validation.
pub async fn set_policy<F>(
    pool: &SqlitePool,
    mut input: PolicyInput,
    now: DateTime<Utc>,
    key_id: &str,
    public_key: &str,
    sign: F,
) -> Result<Policy, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let invalid_input = || invalid("invalid private application policy");
    let mut records = BTreeSet::new();
    for effect in &mut input.effects {
        effect.formula = validate(&effect.formula).map_err(invalid)?;
        if effect.record.is_empty() || !records.insert(effect.record.as_str()) {
            return Err(invalid("each private effect needs a different Record"));
        }
    }
    if input.transfer.is_empty()
        || input.exchange.is_empty()
        || input.person.is_empty()
        || input.effects.len() > 32
        || key_id.is_empty()
        || public_key.is_empty()
        || input.request_id.trim().is_empty()
        || input.request_id.len() > 200
    {
        return Err(invalid_input());
    }
    let primary = input.effects.first().ok_or_else(invalid_input)?;
    let version = i64::try_from(input.expected_version)
        .ok()
        .and_then(|version| version.checked_add(1))
        .ok_or_else(invalid_input)?;
    let payload =
        serde_json::to_string(&input).map_err(|error| StoreError::Encode(Box::new(error)))?;
    let hash = sha256_hex(payload.as_bytes());
    let mut tx = write_tx(pool).await?;
    require_private_writer(&mut tx, &input.transfer, &input.person).await?;
    if let Some(row) =
        sqlx::query("SELECT * FROM transfer_private_policy_event WHERE request_id = ?")
            .bind(&input.request_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if row.get::<&str, _>("payload") != payload {
            return Err(invalid(
                "private policy request was reused with different values",
            ));
        }
        tx.rollback().await?;
        return map_policy(row);
    }
    let mut admission = QueryBuilder::<Sqlite>::new(
        "SELECT EXISTS(
            SELECT 1 FROM record person
            JOIN record organ ON organ.slug = 'local-organ' AND organ.kind = 'organ'
            WHERE person.uid = ",
    );
    admission.push_bind(&input.person).push(
        " AND person.kind = 'person' AND person.organ_uid = organ.uid
          AND person.deleted_at IS NULL AND organ.deleted_at IS NULL
          AND (SELECT COUNT(*) FROM record
               WHERE organ_uid = organ.uid AND deleted_at IS NULL AND uid IN (",
    );
    let mut targets = admission.separated(", ");
    for effect in &input.effects {
        targets.push_bind(&effect.record);
    }
    admission
        .push(")) = ")
        .push_bind(u32::try_from(input.effects.len()).map_err(invalid)?)
        .push(")");
    let local: bool = admission.build_query_scalar().fetch_one(&mut *tx).await?;
    if !local {
        return Err(invalid(
            "private policies require this Organ's Person and Record",
        ));
    }
    let previous: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT record_uid, mode FROM transfer_private_effect
         WHERE transfer_uid = ? AND exchange_uid = ? AND person_uid = ?",
    )
    .bind(&input.transfer)
    .bind(&input.exchange)
    .bind(&input.person)
    .fetch_all(&mut *tx)
    .await?;
    if !previous.is_empty()
        && (previous.len() != input.effects.len()
            || previous.iter().any(|(record, mode)| {
                !input.effects.iter().any(|effect| {
                    effect.record == *record && mode == transfer_effects::mode(effect.mode)
                })
            }))
    {
        return Err(invalid(
            "a partially applied outcome must keep its Records and effect kinds",
        ));
    }
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT e.version FROM transfer_private_policy p
         JOIN transfer_private_policy_event e ON e.uid = p.event_uid
         WHERE p.transfer_uid = ? AND p.exchange_uid = ? AND p.person_uid = ?",
    )
    .bind(&input.transfer)
    .bind(&input.exchange)
    .bind(&input.person)
    .fetch_optional(&mut *tx)
    .await?;
    if current.unwrap_or(0) != version - 1 {
        return Err(invalid("private application policy changed after review"));
    }
    let signature =
        sign(&hash).ok_or_else(|| invalid("private policy requires the owner's signature"))?;
    let policy = Policy {
        uid: new_uid("tpp"),
        transfer: input.transfer,
        exchange: input.exchange,
        person: input.person,
        record: primary.record.clone(),
        formula: primary.formula.clone(),
        effects: input.effects,
        version: version.cast_unsigned(),
        hash,
    };
    sqlx::query(
        "INSERT INTO transfer_private_policy_event
            (uid, transfer_uid, exchange_uid, person_uid, record_uid, version, formula,
             policy_hash, payload, signature, request_id, created_at, key_id, public_key, effects_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&policy.uid)
    .bind(&policy.transfer)
    .bind(&policy.exchange)
    .bind(&policy.person)
    .bind(&policy.record)
    .bind(version)
    .bind(&policy.formula)
    .bind(&policy.hash)
    .bind(payload)
    .bind(signature)
    .bind(&input.request_id)
    .bind(now.to_rfc3339())
    .bind(key_id)
    .bind(public_key)
    .bind(serde_json::to_string(&policy.effects).map_err(invalid)?)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_private_policy (transfer_uid, exchange_uid, person_uid, event_uid)
         VALUES (?, ?, ?, ?) ON CONFLICT (transfer_uid, exchange_uid, person_uid)
         DO UPDATE SET event_uid = excluded.event_uid",
    )
    .bind(&policy.transfer)
    .bind(&policy.exchange)
    .bind(&policy.person)
    .bind(&policy.uid)
    .execute(&mut *tx)
    .await?;
    validate_all_on(&mut tx).await?;
    tx.commit().await?;
    Ok(policy)
}

/// Read the current policy using the caller's database connection.
///
/// # Errors
///
/// Returns an error if the query fails or the saved effects cannot be decoded.
pub async fn policy_on(
    connection: &mut SqliteConnection,
    transfer: &str,
    exchange: &str,
    person: &str,
) -> Result<Option<Policy>, StoreError> {
    sqlx::query(
        "SELECT e.* FROM transfer_private_policy p
         JOIN transfer_private_policy_event e ON e.uid = p.event_uid
         WHERE p.transfer_uid = ? AND p.exchange_uid = ? AND p.person_uid = ?",
    )
    .bind(transfer)
    .bind(exchange)
    .bind(person)
    .try_map(map_policy)
    .fetch_optional(connection)
    .await
}

/// Return the policy's primary Record, or the supplied fallback.
///
/// # Errors
///
/// Returns an error if the current policy cannot be read or decoded.
pub async fn bound_record(
    pool: &SqlitePool,
    transfer: &str,
    exchange: &str,
    person: &str,
    fallback: Option<&str>,
) -> Result<Option<String>, StoreError> {
    Ok(policy(pool, transfer, exchange, person)
        .await?
        .map(|policy| policy.record)
        .or_else(|| fallback.map(str::to_string)))
}
