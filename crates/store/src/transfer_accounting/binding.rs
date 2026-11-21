//! Resolve a reviewed Record, unit, and formula against the current Transfer state.

use nucleus::{
    fact::sha256_hex,
    transfer::{
        application::{EffectMode, validate},
        occurrence_application_formula_hash,
    },
};
use serde::Serialize;
use serde_json::{json, to_vec};
use sqlx::{SqliteConnection, SqlitePool};

use super::{Policy, invalid, policy_on};
use crate::{
    StoreError, concepts::conversion_ratio_on, transfer_delivery::RemoteApplicationHandoffRow,
};

/// The public Transfer context and the private Record being reviewed.
#[derive(Debug)]
pub struct Binding<'a> {
    /// Transfer identifier.
    pub transfer: &'a str,
    /// Exchange whose policy applies.
    pub exchange: &'a str,
    /// Occurrence supplying legacy policy and prior applications, when known.
    pub occurrence: Option<&'a str>,
    /// Person whose policy and applications apply.
    pub person: &'a str,
    /// Reviewed destination when known.
    pub record: Option<&'a str>,
    /// Unit of the canonical quantity, if present.
    pub unit: Option<&'a str>,
    /// Use the outgoing default where no policy applies.
    pub outgoing: bool,
}

/// Resolved application formula and the evidence used to review it.
#[derive(Clone, Debug, Serialize)]
pub struct Effective {
    /// Resolved private Record, if any.
    pub record: Option<String>,
    /// Validated formula including any canonical-to-local conversion.
    pub formula: String,
    /// Hash covering the formula and any policy or unit binding.
    pub formula_hash: String,
    /// Policy revision, or zero for a default.
    pub version: u64,
    /// Explicit private policy, when one exists.
    pub policy: Option<Policy>,
    /// Unit of the resulting local quantity, if present.
    pub unit: Option<String>,
}

/// Resolve the effective formula and Record for a reviewed application.
///
/// # Errors
///
/// Returns an error if the Record or unit changed after review or partial
/// application, the units cannot be converted, or the policy or read fails.
pub async fn effective(pool: &SqlitePool, binding: &Binding<'_>) -> Result<Effective, StoreError> {
    effective_on(&mut *pool.acquire().await?, binding).await
}

pub(crate) async fn effective_on(
    connection: &mut SqliteConnection,
    binding: &Binding<'_>,
) -> Result<Effective, StoreError> {
    let policy = policy_on(
        connection,
        binding.transfer,
        binding.exchange,
        binding.person,
    )
    .await?;
    effective_with_policy_on(connection, binding, policy).await
}

pub(crate) async fn effective_with_policy_on(
    connection: &mut SqliteConnection,
    binding: &Binding<'_>,
    policy: Option<Policy>,
) -> Result<Effective, StoreError> {
    let legacy;
    let configured: Option<String>;
    let (formula, version) = if let Some(policy) = &policy {
        (policy.formula.as_str(), policy.version)
    } else if let Some((formula, version)) = sqlx::query_as::<_, (String, i64)>(
        "SELECT formula, version FROM transfer_occurrence_application_policy
         WHERE occurrence_uid = ? AND receiver_person_uid = ?",
    )
    .bind(binding.occurrence)
    .bind(binding.person)
    .fetch_optional(&mut *connection)
    .await?
    {
        legacy = formula;
        (legacy.as_str(), version.cast_unsigned())
    } else if binding.outgoing {
        ("-incoming()", 0)
    } else {
        configured = sqlx::query_scalar(
            "SELECT transfer_application_formula FROM configuration WHERE id = 1",
        )
        .fetch_optional(&mut *connection)
        .await?;
        (configured.as_deref().unwrap_or("incoming()"), 0)
    };
    let record = policy
        .as_ref()
        .map(|policy| policy.record.as_str())
        .or(binding.record);
    if binding.record.is_some() && record != binding.record {
        return Err(invalid(
            "the reviewed Record differs from the owner's private policy",
        ));
    }
    if let (Some(occurrence), Some(record)) = (binding.occurrence, record) {
        require_same_record(connection, occurrence, binding.person, record).await?;
    }
    let unit: Option<String> = if let Some(record) = record {
        sqlx::query_scalar("SELECT unit_uid FROM record WHERE uid = ? AND deleted_at IS NULL")
            .bind(record)
            .fetch_optional(&mut *connection)
            .await?
            .ok_or_else(|| invalid("the private application Record is unavailable"))?
    } else {
        binding.unit.map(str::to_string)
    };
    let mut formula = validate(formula).map_err(invalid)?;
    if binding.unit != unit.as_deref()
        && policy
            .as_ref()
            .and_then(|policy| policy.effects.first())
            .is_none_or(|effect| effect.mode == EffectMode::Quantity)
    {
        formula.retain(|character| !character.is_whitespace());
        let (Some(from), Some(to)) = (binding.unit, unit.as_deref()) else {
            return Err(invalid(
                "public and private quantities need compatible units",
            ));
        };
        let (numerator, denominator) = conversion_ratio_on(connection, from, to)
            .await?
            .ok_or_else(|| {
                invalid("no compatible unit conversion exists for this private Record")
            })?;
        formula = validate(&formula.replace(
            "incoming()",
            &format!("(incoming() * {numerator} / {denominator})"),
        ))
        .map_err(invalid)?;
    }
    let formula_hash = if policy.is_some() || binding.unit != unit.as_deref() {
        sha256_hex(
            &to_vec(&json!({
                "formula": formula,
                "policy": policy.as_ref().map(|value| &value.hash),
                "record": record,
                "canonical_unit": binding.unit,
                "local_unit": unit,
            }))
            .map_err(invalid)?,
        )
    } else {
        occurrence_application_formula_hash(&formula)
    };
    Ok(Effective {
        record: record.map(str::to_string),
        formula,
        formula_hash,
        version,
        policy,
        unit,
    })
}

/// Resolve the current exchange for an application handoff.
///
/// # Errors
///
/// Returns an error if the exchange is unavailable or the lookup fails.
pub async fn handoff_exchange(
    pool: &SqlitePool,
    handoff: &RemoteApplicationHandoffRow,
) -> Result<String, StoreError> {
    exchange_for_promise(
        pool,
        &handoff.transfer_uid,
        &handoff.source_promise_uid,
        &handoff.participant_person_uid,
    )
    .await
}

/// Find the Promise's exchange from the local or active remote Transfer.
///
/// # Errors
///
/// Returns an error if neither source provides an exchange or a query fails.
pub async fn exchange_for_promise(
    pool: &SqlitePool,
    transfer: &str,
    promise: &str,
    person: &str,
) -> Result<String, StoreError> {
    if let Some(exchange) = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(json_extract(item_json, '$.exchange.uid'), uid)
         FROM promise WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(promise)
    .bind(transfer)
    .fetch_optional(pool)
    .await?
    {
        return Ok(exchange);
    }
    sqlx::query_scalar(
        "SELECT json_extract(p.value, '$.exchange')
         FROM transfer_remote_reference r, json_each(r.projection, '$.promises') p
         WHERE r.transfer_uid = ? AND r.recipient_person_uid = ? AND r.state = 'active'
           AND json_extract(p.value, '$.uid') = ?
         ORDER BY r.last_transfer_revision DESC LIMIT 1",
    )
    .bind(transfer)
    .bind(person)
    .bind(promise)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| invalid("the current Transfer exchange is unavailable"))
}

pub(crate) async fn require_same_record(
    connection: &mut SqliteConnection,
    occurrence: &str,
    person: &str,
    record: &str,
) -> Result<(), StoreError> {
    let changed: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM transfer_occurrence_settlement_slice s
            JOIN fact f ON f.uid = s.application_fact_uid
            WHERE s.occurrence_uid = ?1 AND s.owner_person_uid = ?2
              AND (s.local_record_uid != ?3 OR json_extract(f.payload, '$.local_unit_uid')
                   IS NOT (SELECT unit_uid FROM record WHERE uid = ?3))
            UNION ALL
            SELECT 1 FROM transfer_local_application a
            JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid
            JOIN fact f ON f.uid = a.application_fact_uid
            WHERE h.occurrence_uid = ?1 AND h.participant_person_uid = ?2
              AND (a.local_record_uid != ?3 OR json_extract(f.payload, '$.local_unit_uid')
                   IS NOT (SELECT unit_uid FROM record WHERE uid = ?3)))",
    )
    .bind(occurrence)
    .bind(person)
    .bind(record)
    .fetch_one(connection)
    .await?;
    if changed {
        return Err(invalid(
            "a partial application must retain its original private Record and unit",
        ));
    }
    Ok(())
}
