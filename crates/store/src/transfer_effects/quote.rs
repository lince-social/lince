//! Quotes the private Record effects of a reviewed transfer amount.

use std::collections::HashMap;

use nucleus::{
    DecimalValue,
    fact::sha256_hex,
    transfer::application::{EffectMode, validate},
};
use serde_json::{json, to_vec};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{Effect, Group, invalid, mode};
use crate::{
    StoreError, exact, transfer_accounting as accounting,
    transfer_loans::{self, Adjustment},
};

/// Quotes the effect group in a consistent read transaction.
///
/// # Errors
///
/// Returns an error if the policy, Record units, calculations, or storage fail.
pub async fn quote(
    pool: &SqlitePool,
    binding: &accounting::Binding<'_>,
    canonical_after: DecimalValue,
) -> Result<Option<Group>, StoreError> {
    let mut tx = pool.begin().await?;
    let result = quote_on(&mut tx, binding, canonical_after).await?;
    tx.rollback().await?;
    Ok(result)
}

/// Quotes the effect group on the caller's connection.
///
/// # Errors
///
/// Returns an error if the policy, Record units, calculations, or storage fail.
pub async fn quote_on(
    connection: &mut SqliteConnection,
    binding: &accounting::Binding<'_>,
    canonical_after: DecimalValue,
) -> Result<Option<Group>, StoreError> {
    let application = accounting::effective_on(connection, binding).await?;
    quote_effective_on(connection, binding, canonical_after, &application).await
}

pub(crate) async fn quote_effective_on(
    connection: &mut SqliteConnection,
    binding: &accounting::Binding<'_>,
    canonical_after: DecimalValue,
    application: &accounting::Effective,
) -> Result<Option<Group>, StoreError> {
    if application.policy.is_none() {
        return Ok(None);
    }
    let adjustments =
        transfer_loans::adjustments_on(connection, nucleus::execution::now().timestamp_millis())
            .await?;
    let availability: HashMap<_, _> = adjustments
        .iter()
        .map(|adjustment| (adjustment.record.as_str(), adjustment))
        .collect();
    quote_prepared_on(
        connection,
        binding,
        canonical_after,
        application,
        None,
        |record| availability.get(record).copied(),
    )
    .await
}

pub(crate) async fn assumptions_on(connection: &mut SqliteConnection) -> Result<bool, StoreError> {
    if nucleus::execution::current().is_none() {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema
                       WHERE type = 'table' AND name = 'simulation_assumed_transfer_effect')",
    )
    .fetch_one(connection)
    .await
}

pub(crate) async fn quote_prepared_on<'a>(
    connection: &mut SqliteConnection,
    binding: &accounting::Binding<'_>,
    canonical_after: DecimalValue,
    application: &accounting::Effective,
    cached_assumptions: Option<bool>,
    adjustment_for: impl Fn(&str) -> Option<&'a Adjustment>,
) -> Result<Option<Group>, StoreError> {
    let Some(policy) = &application.policy else {
        return Ok(None);
    };
    let previous = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT DISTINCT record_uid, mode, unit_uid FROM transfer_private_effect
         WHERE transfer_uid = ?1 AND exchange_uid = ?2 AND (?3 IS NULL OR occurrence_uid = ?3)
           AND person_uid = ?4 ORDER BY record_uid",
    )
    .bind(binding.transfer)
    .bind(binding.exchange)
    .bind(binding.occurrence)
    .bind(binding.person)
    .fetch_all(&mut *connection)
    .await?;
    if !previous.is_empty()
        && (previous.len() != policy.effects.len()
            || previous.iter().any(|(record, previous_mode, _)| {
                !policy
                    .effects
                    .iter()
                    .any(|effect| effect.record == *record && mode(effect.mode) == previous_mode)
            }))
    {
        return Err(invalid(
            "a partially applied outcome must keep its Records and effect kinds",
        ));
    }
    let has_assumptions = match cached_assumptions {
        Some(assumptions) => assumptions,
        None => assumptions_on(connection).await?,
    };
    let mut effects = Vec::with_capacity(policy.effects.len());
    for (index, configured) in policy.effects.iter().enumerate() {
        let row = sqlx::query(
            "SELECT r.head, r.quantity_mantissa, r.quantity_scale, r.unit_uid FROM record r
             JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ'
                 AND own.kind = 'organ' AND own.deleted_at IS NULL
             WHERE r.uid = ? AND r.deleted_at IS NULL",
        )
        .bind(&configured.record)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| invalid("every private effect needs a live Record on this Organ"))?;
        let adjustment = adjustment_for(&configured.record);
        if adjustment.is_some_and(|adjustment| adjustment.unit_changed) {
            return Err(invalid("loan availability needs the original Record unit"));
        }
        let before = exact::read_decimal(&row, "quantity")?;
        let available_before = exact::sum_exact([
            before,
            adjustment.map_or_else(exact::zero, |adjustment| adjustment.delta),
        ])?;
        let unit: Option<String> = row.get("unit_uid");
        if previous.iter().any(|(record, _, original_unit)| {
            record == &configured.record && original_unit != &unit
        }) {
            return Err(invalid(
                "a partial private effect must retain its original unit",
            ));
        }
        let mut formula = if index == 0 {
            application.formula.clone()
        } else {
            configured.formula.clone()
        };
        if index != 0 && configured.mode == EffectMode::Quantity && binding.unit != unit.as_deref()
        {
            let (Some(from), Some(to)) = (binding.unit, unit.as_deref()) else {
                return Err(invalid(
                    "quantity effects need compatible units; use a fulfilment link for a different Need",
                ));
            };
            let (numerator, denominator) =
                crate::concepts::conversion_ratio_on(connection, from, to)
                    .await?
                    .ok_or_else(|| {
                        invalid("no compatible unit conversion exists for this private effect")
                    })?;
            formula.retain(|character| !character.is_whitespace());
            formula = formula.replace(
                "incoming()",
                &format!("(incoming() * {numerator} / {denominator})"),
            );
        }
        formula = validate(&formula).map_err(invalid)?;
        let rows = sqlx::query(
            "SELECT f.delta_mantissa, f.delta_scale FROM transfer_private_effect e
             JOIN fact f ON f.uid = e.fact_uid
             WHERE e.transfer_uid = ?1 AND e.exchange_uid = ?2
               AND (?3 IS NULL OR e.occurrence_uid = ?3) AND e.person_uid = ?4 AND e.record_uid = ?5",
        )
        .bind(binding.transfer)
        .bind(binding.exchange)
        .bind(binding.occurrence)
        .bind(binding.person)
        .bind(&configured.record)
        .fetch_all(&mut *connection)
        .await?;
        let mut cumulative_before = if index == 0 && rows.is_empty() {
            match binding.occurrence {
                Some(occurrence) => {
                    accounting::applied_on(connection, occurrence, binding.person)
                        .await?
                        .local
                }
                None => exact::zero(),
            }
        } else {
            exact::sum_exact(
                rows.iter()
                    .map(|row| exact::read_decimal(row, "delta"))
                    .collect::<Result<Vec<_>, _>>()?,
            )?
        };
        if has_assumptions {
            let assumed = sqlx::query(
                "SELECT local_mantissa, local_scale FROM simulation_assumed_transfer_effect
                 WHERE transfer_uid = ?1 AND exchange_uid = ?2 AND person_uid = ?3 AND record_uid = ?4
                   AND (?5 IS NULL OR occurrence_uid IS ?5 OR occurrence_uid IS NULL)",
            )
            .bind(binding.transfer)
            .bind(binding.exchange)
            .bind(binding.person)
            .bind(&configured.record)
            .bind(binding.occurrence)
            .fetch_all(&mut *connection)
            .await?;
            for row in assumed {
                cumulative_before =
                    exact::sum_exact([cumulative_before, exact::read_decimal(&row, "local")?])?;
            }
        }
        let (mut delta, _) = accounting::calculate(&formula, canonical_after, cumulative_before)?;
        if configured.mode == EffectMode::Fulfilment {
            if delta.mantissa() < 0 {
                return Err(invalid("a fulfilment may only meet an outstanding Need"));
            }
            let needed = if available_before.mantissa() < 0 {
                exact::negate(available_before)?
            } else {
                exact::zero()
            };
            if delta.exact_numeric_cmp(needed).is_gt() {
                delta = needed;
            }
        }
        effects.push(Effect {
            record: configured.record.clone(),
            title: row.get("head"),
            mode: configured.mode,
            unit,
            formula,
            before,
            available_before,
            delta,
            after: exact::sum_exact([before, delta])?,
            cumulative_before,
            cumulative_after: exact::sum_exact([cumulative_before, delta])?,
        });
    }
    let review =
        json!({"policy": policy.hash, "canonical_after": canonical_after, "effects": effects});
    let hash = sha256_hex(&to_vec(&review).map_err(invalid)?);
    Ok(Some(Group {
        transfer: binding.transfer.into(),
        exchange: binding.exchange.into(),
        effects,
        hash,
    }))
}

/// Checks that the current group matches the caller's reviewed hash.
///
/// # Errors
///
/// Returns an error if the group was added, removed, or changed.
pub fn require_review(group: Option<&Group>, reviewed: Option<&str>) -> Result<(), StoreError> {
    if group.map(|group| group.hash.as_str()) != reviewed {
        return Err(invalid("the private effect group changed after review"));
    }
    Ok(())
}
