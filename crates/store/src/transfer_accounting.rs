use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::StoreError;
use nucleus::transfer::application::{EffectMode, PrivateEffect};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub uid: String,
    pub transfer: String,
    pub exchange: String,
    pub person: String,
    pub record: String,
    pub formula: String,
    pub effects: Vec<PrivateEffect>,
    pub version: u64,
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    pub transfer: String,
    pub exchange: String,
    pub person: String,
    pub effects: Vec<PrivateEffect>,
    pub expected_version: u64,
    pub request_id: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> Result<Policy, StoreError> {
    let mut effects: Vec<PrivateEffect> =
        serde_json::from_str(&row.get::<String, _>("effects_json")).map_err(invalid)?;
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
        version: row.get::<i64, _>("version") as u64,
        hash: row.get("policy_hash"),
    })
}

pub async fn policy(
    pool: &SqlitePool,
    transfer: &str,
    exchange: &str,
    person: &str,
) -> Result<Option<Policy>, StoreError> {
    sqlx::query("SELECT e.* FROM transfer_private_policy p JOIN transfer_private_policy_event e ON e.uid = p.event_uid WHERE p.transfer_uid = ? AND p.exchange_uid = ? AND p.person_uid = ?")
        .bind(transfer).bind(exchange).bind(person).fetch_optional(pool).await?.map(map).transpose()
}

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
    let mut records = std::collections::BTreeSet::new();
    for effect in &mut input.effects {
        effect.formula =
            nucleus::transfer::application::validate(&effect.formula).map_err(invalid)?;
        if effect.record.is_empty() || !records.insert(effect.record.clone()) {
            return Err(invalid("each private effect needs a different Record"));
        }
    }
    if input.transfer.is_empty()
        || input.exchange.is_empty()
        || input.person.is_empty()
        || input.effects.is_empty()
        || input.effects.len() > 32
        || key_id.is_empty()
        || public_key.is_empty()
        || input.request_id.trim().is_empty()
        || input.request_id.len() > 200
        || input.expected_version >= i64::MAX as u64
    {
        return Err(StoreError::Protocol(
            "invalid private application policy".into(),
        ));
    }
    let payload =
        serde_json::to_string(&input).map_err(|error| StoreError::Encode(Box::new(error)))?;
    let hash = nucleus::fact::sha256_hex(payload.as_bytes());
    let mut tx = crate::write_tx(pool).await?;
    crate::transfer_replication::require_private_writer(&mut tx, &input.transfer, &input.person).await?;
    if let Some(row) =
        sqlx::query("SELECT * FROM transfer_private_policy_event WHERE request_id = ?")
            .bind(&input.request_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if row.get::<String, _>("payload") != payload {
            return Err(StoreError::Protocol(
                "private policy request was reused with different values".into(),
            ));
        }
        tx.rollback().await?;
        return map(row);
    }
    for effect in &input.effects {
        let local: bool = sqlx::query_scalar(
            "SELECT EXISTS(
        SELECT 1 FROM record person JOIN record resource ON resource.uid = ?
        JOIN record organ ON organ.slug = 'local-organ' AND organ.kind = 'organ'
        WHERE person.uid = ? AND person.kind = 'person' AND person.organ_uid = organ.uid
          AND resource.organ_uid = organ.uid AND person.deleted_at IS NULL
          AND resource.deleted_at IS NULL AND organ.deleted_at IS NULL)",
        )
        .bind(&effect.record)
        .bind(&input.person)
        .fetch_one(&mut *tx)
        .await?;
        if !local {
            return Err(StoreError::Protocol(
                "private policies require this Organ's Person and Record".into(),
            ));
        }
    }
    let previous: Vec<(String,String)> = sqlx::query_as("SELECT DISTINCT record_uid, mode FROM transfer_private_effect WHERE transfer_uid = ? AND exchange_uid = ? AND person_uid = ?")
        .bind(&input.transfer).bind(&input.exchange).bind(&input.person).fetch_all(&mut *tx).await?;
    if !previous.is_empty()
        && (previous.len() != input.effects.len()
            || previous.iter().any(|(record, mode)| {
                !input.effects.iter().any(|effect| {
                    effect.record == *record
                        && match effect.mode {
                            EffectMode::Quantity => mode == "quantity",
                            EffectMode::Fulfilment => mode == "fulfilment",
                        }
                })
            }))
    {
        return Err(invalid(
            "a partially applied outcome must keep its Records and effect kinds",
        ));
    }
    let current: Option<i64> = sqlx::query_scalar("SELECT e.version FROM transfer_private_policy p JOIN transfer_private_policy_event e ON e.uid = p.event_uid WHERE p.transfer_uid = ? AND p.exchange_uid = ? AND p.person_uid = ?")
        .bind(&input.transfer).bind(&input.exchange).bind(&input.person).fetch_optional(&mut *tx).await?;
    if current.unwrap_or(0) as u64 != input.expected_version {
        return Err(StoreError::Protocol(
            "private application policy changed after review".into(),
        ));
    }
    let signature = sign(&hash).ok_or_else(|| {
        StoreError::Protocol("private policy requires the owner's signature".into())
    })?;
    let policy = Policy {
        uid: nucleus::new_uid("tpp"),
        transfer: input.transfer,
        exchange: input.exchange,
        person: input.person,
        record: input.effects[0].record.clone(),
        formula: input.effects[0].formula.clone(),
        effects: input.effects,
        version: input.expected_version + 1,
        hash,
    };
    sqlx::query("INSERT INTO transfer_private_policy_event (uid, transfer_uid, exchange_uid, person_uid, record_uid, version, formula, policy_hash, payload, signature, request_id, created_at, key_id, public_key, effects_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&policy.uid).bind(&policy.transfer).bind(&policy.exchange).bind(&policy.person).bind(&policy.record)
        .bind(policy.version as i64).bind(&policy.formula).bind(&policy.hash).bind(payload).bind(signature)
        .bind(&input.request_id).bind(now.to_rfc3339()).bind(key_id).bind(public_key)
        .bind(serde_json::to_string(&policy.effects).map_err(invalid)?).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO transfer_private_policy (transfer_uid, exchange_uid, person_uid, event_uid) VALUES (?, ?, ?, ?) ON CONFLICT (transfer_uid, exchange_uid, person_uid) DO UPDATE SET event_uid = excluded.event_uid")
        .bind(&policy.transfer).bind(&policy.exchange).bind(&policy.person).bind(&policy.uid).execute(&mut *tx).await?;
    crate::transfer_stock::validate_all_on(&mut tx).await?;
    tx.commit().await?;
    Ok(policy)
}

pub async fn policy_on(
    connection: &mut sqlx::SqliteConnection,
    transfer: &str,
    exchange: &str,
    person: &str,
) -> Result<Option<Policy>, StoreError> {
    sqlx::query("SELECT e.* FROM transfer_private_policy p JOIN transfer_private_policy_event e ON e.uid = p.event_uid WHERE p.transfer_uid = ? AND p.exchange_uid = ? AND p.person_uid = ?")
        .bind(transfer).bind(exchange).bind(person).fetch_optional(connection).await?.map(map).transpose()
}

pub struct Binding<'a> {
    pub transfer: &'a str,
    pub exchange: &'a str,
    pub occurrence: Option<&'a str>,
    pub person: &'a str,
    pub record: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub outgoing: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Effective {
    pub record: Option<String>,
    pub formula: String,
    pub formula_hash: String,
    pub version: u64,
    pub policy: Option<Policy>,
    pub unit: Option<String>,
}

fn invalid(error: impl std::fmt::Display) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub async fn effective(pool: &SqlitePool, binding: &Binding<'_>) -> Result<Effective, StoreError> {
    effective_on(&mut *pool.acquire().await?, binding).await
}

pub(crate) async fn effective_on(
    connection: &mut sqlx::SqliteConnection,
    binding: &Binding<'_>,
) -> Result<Effective, StoreError> {
    let policy = sqlx::query("SELECT e.* FROM transfer_private_policy p JOIN transfer_private_policy_event e ON e.uid = p.event_uid WHERE p.transfer_uid = ? AND p.exchange_uid = ? AND p.person_uid = ?")
        .bind(binding.transfer).bind(binding.exchange).bind(binding.person)
        .fetch_optional(&mut *connection).await?.map(map).transpose()?;
    let legacy: Option<(String, i64)> = if policy.is_none() {
        sqlx::query_as("SELECT formula, version FROM transfer_occurrence_application_policy WHERE occurrence_uid = ? AND receiver_person_uid = ?")
            .bind(binding.occurrence).bind(binding.person).fetch_optional(&mut *connection).await?
    } else {
        None
    };
    let (mut formula, version) = if let Some(policy) = &policy {
        (policy.formula.clone(), policy.version)
    } else if let Some((formula, version)) = legacy {
        (formula, version as u64)
    } else if binding.outgoing {
        ("-incoming()".into(), 0)
    } else {
        (
            sqlx::query_scalar(
                "SELECT transfer_application_formula FROM configuration WHERE id = 1",
            )
            .fetch_optional(&mut *connection)
            .await?
            .unwrap_or_else(|| "incoming()".to_string()),
            0,
        )
    };
    let record = policy
        .as_ref()
        .map(|policy| policy.record.as_str())
        .or(binding.record);
    if policy.is_some() && binding.record.is_some() && record != binding.record {
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
    formula = nucleus::transfer::application::validate(&formula).map_err(invalid)?;
    if binding.unit != unit.as_deref()
        && policy
            .as_ref()
            .is_none_or(|policy| policy.effects[0].mode == EffectMode::Quantity)
    {
        formula.retain(|character| !character.is_whitespace());
        let (Some(from), Some(to)) = (binding.unit, unit.as_deref()) else {
            return Err(invalid(
                "public and private quantities need compatible units",
            ));
        };
        let (numerator, denominator) = crate::concepts::conversion_ratio_on(connection, from, to)
            .await?
            .ok_or_else(|| {
                invalid("no compatible unit conversion exists for this private Record")
            })?;
        formula = formula.replace(
            "incoming()",
            &format!("(incoming() * {numerator} / {denominator})"),
        );
    }
    formula = nucleus::transfer::application::validate(&formula).map_err(invalid)?;
    let formula_hash = if policy.is_some() || binding.unit != unit.as_deref() {
        nucleus::fact::sha256_hex(
            serde_json::to_string(&serde_json::json!({
                "formula":formula,"policy":policy.as_ref().map(|value| &value.hash),
                "record":record,"canonical_unit":binding.unit,"local_unit":unit
            }))
            .map_err(invalid)?
            .as_bytes(),
        )
    } else {
        nucleus::transfer::occurrence_application_formula_hash(&formula)
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

pub async fn handoff_exchange(
    pool: &SqlitePool,
    handoff: &crate::transfer_delivery::RemoteApplicationHandoffRow,
) -> Result<String, StoreError> {
    exchange_for_promise(
        pool,
        &handoff.transfer_uid,
        &handoff.source_promise_uid,
        &handoff.participant_person_uid,
    )
    .await
}

pub async fn exchange_for_promise(
    pool: &SqlitePool,
    transfer: &str,
    promise: &str,
    person: &str,
) -> Result<String, StoreError> {
    if let Some(exchange) = sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(json_extract(item_json, '$.exchange.uid'),uid) FROM promise WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(promise)
    .bind(transfer)
    .fetch_optional(pool)
    .await?
    {
        return Ok(exchange);
    }
    sqlx::query_scalar("SELECT json_extract(p.value, '$.exchange') FROM transfer_remote_reference r, json_each(r.projection, '$.promises') p WHERE r.transfer_uid = ? AND r.recipient_person_uid = ? AND r.state = 'active' AND json_extract(p.value, '$.uid') = ? ORDER BY r.last_transfer_revision DESC LIMIT 1")
        .bind(transfer).bind(person).bind(promise).fetch_optional(pool).await?
        .ok_or_else(|| invalid("the current Transfer exchange is unavailable"))
}

#[derive(Clone, Debug)]
pub struct Applied {
    pub canonical: nucleus::DecimalValue,
    pub local: nucleus::DecimalValue,
}

pub async fn applied(
    pool: &SqlitePool,
    occurrence: &str,
    person: &str,
) -> Result<Applied, StoreError> {
    applied_on(&mut *pool.acquire().await?, occurrence, person).await
}

pub(crate) async fn applied_on(
    connection: &mut sqlx::SqliteConnection,
    occurrence: &str,
    person: &str,
) -> Result<Applied, StoreError> {
    let rows = sqlx::query("SELECT s.canonical_quantity, f.delta_mantissa, f.delta_scale FROM transfer_occurrence_settlement_slice s JOIN fact f ON f.uid = s.application_fact_uid WHERE s.occurrence_uid = ? AND s.owner_person_uid = ? UNION ALL SELECT h.canonical_quantity, f.delta_mantissa, f.delta_scale FROM transfer_local_application a JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid JOIN fact f ON f.uid = a.application_fact_uid WHERE h.occurrence_uid = ? AND h.participant_person_uid = ?")
        .bind(occurrence).bind(person).bind(occurrence).bind(person).fetch_all(&mut *connection).await?;
    let canonical = rows
        .iter()
        .map(|row| {
            nucleus::transfer::application::amount(row.get("canonical_quantity")).map_err(invalid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let local = rows
        .iter()
        .map(|row| crate::exact::read_decimal(row, "delta"))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Applied {
        canonical: crate::exact::sum_exact(canonical)?,
        local: crate::exact::sum_exact(local)?,
    })
}

pub fn calculate(
    formula: &str,
    canonical: nucleus::DecimalValue,
    applied: nucleus::DecimalValue,
) -> Result<(nucleus::DecimalValue, nucleus::DecimalValue), StoreError> {
    let after =
        nucleus::transfer::application::evaluate_exact(formula, canonical).map_err(invalid)?;
    Ok((crate::exact::difference(after, applied)?, after))
}

pub fn amount(value: f64) -> Result<nucleus::DecimalValue, StoreError> {
    nucleus::transfer::application::amount(value).map_err(invalid)
}

pub async fn allocated(
    pool: &SqlitePool,
    occurrence: &str,
    include_pending: bool,
) -> Result<nucleus::DecimalValue, StoreError> {
    allocated_on(&mut *pool.acquire().await?, occurrence, include_pending).await
}

pub(crate) async fn allocated_on(
    connection: &mut sqlx::SqliteConnection,
    occurrence: &str,
    include_pending: bool,
) -> Result<nucleus::DecimalValue, StoreError> {
    let values: Vec<f64> = sqlx::query_scalar("SELECT canonical_quantity FROM transfer_occurrence_settlement_slice WHERE occurrence_uid = ? UNION ALL SELECT d.canonical_quantity FROM transfer_application_handoff h JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid JOIN promise p ON p.uid = d.source_promise_uid AND p.party_uid = h.participant_person_uid WHERE h.occurrence_uid = ? AND (h.state = 'accepted' OR (? AND h.state = 'pending'))")
        .bind(occurrence).bind(occurrence).bind(include_pending).fetch_all(&mut *connection).await?;
    crate::exact::sum_exact(
        values
            .into_iter()
            .map(amount)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

pub async fn compensate<F>(
    pool: &SqlitePool,
    application: &str,
    person: &str,
    request: &str,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(nucleus::Fact, bool), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if request.trim().is_empty() || request.len() > 200 {
        return Err(invalid("a correction needs a request identifier"));
    }
    let mut tx = crate::write_tx(pool).await?;
    let row = sqlx::query("SELECT a.*, f.delta_mantissa, f.delta_scale, json_extract(f.payload, '$.local_unit_uid') AS original_unit, r.unit_uid AS current_unit FROM transfer_local_application a JOIN fact f ON f.uid = a.application_fact_uid JOIN record r ON r.uid = a.local_record_uid JOIN record o ON o.slug = 'local-organ' AND o.kind = 'organ' WHERE a.uid = ? AND a.participant_person_uid = ? AND r.organ_uid = o.uid AND r.deleted_at IS NULL")
        .bind(application).bind(person).fetch_optional(&mut *tx).await?
        .ok_or_else(|| invalid("only the owner can correct a local application to a live Record"))?;
    let writer: Option<String> = sqlx::query_scalar("SELECT cell_uid FROM transfer_sync_owner WHERE table_name = 'transfer_local_application' AND row_key = json_array(?) AND cell_uid != (SELECT uid FROM record WHERE slug = 'local-cell')")
        .bind(application).fetch_optional(&mut *tx).await?;
    if let Some(writer) = writer { return Err(invalid(format!("Submit this private correction to its writing Cell {writer}"))); }
    let previous: Option<(String, String, String)> = sqlx::query_as("SELECT application_uid, person_uid, fact_uid FROM transfer_private_application_correction WHERE request_id = ?")
        .bind(request).fetch_optional(&mut *tx).await?;
    if let Some((previous, owner, fact)) = previous {
        if previous != application || owner != person {
            return Err(invalid(
                "correction request was reused with different targets",
            ));
        }
        tx.rollback().await?;
        return Ok((
            crate::facts::get(pool, &fact)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
            true,
        ));
    }
    if row.get::<Option<String>, _>("original_unit") != row.get::<Option<String>, _>("current_unit")
    {
        return Err(invalid(
            "restore the original Record unit before correcting this application",
        ));
    }
    let corrected: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_private_application_correction WHERE application_uid = ?)")
        .bind(application).fetch_one(&mut *tx).await?;
    if corrected {
        return Err(invalid("this private application was already corrected"));
    }
    let record: String = row.get("local_record_uid");
    let original: String = row.get("application_fact_uid");
    let delta = crate::exact::negate(crate::exact::read_decimal(&row, "delta")?)?;
    let hash = crate::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(nucleus::fact::NewFact {
        uid: None, record_uid: record.clone(), delta, at: None, actor_uid: Some(person.into()),
        cause: nucleus::Cause { kind: nucleus::CauseKind::Compensation, uid: Some(original.clone()) },
        payload: Some(serde_json::json!({"action":"compensate-transfer-application","application":application,"original_fact":original,"request_id":request}).to_string()),
    }, &hash, now);
    fact.signature = Some(
        sign(&fact.hash)
            .ok_or_else(|| invalid("a private correction needs the owner's signature"))?,
    );
    crate::facts::insert(&mut tx, &fact).await?;
    crate::records::bump_quantity(&mut tx, &record, delta, &now.to_rfc3339()).await?;
    sqlx::query("INSERT INTO transfer_private_application_correction (uid, application_uid, person_uid, fact_uid, request_id, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(nucleus::new_uid("tpc")).bind(application).bind(person).bind(&fact.uid).bind(request).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    crate::transfer_effects::compensate_on(&mut tx, &original, person, None, now, &sign).await?;
    tx.commit().await?;
    Ok((fact, false))
}

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

pub(crate) async fn require_same_record(
    connection: &mut sqlx::SqliteConnection,
    occurrence: &str,
    person: &str,
    record: &str,
) -> Result<(), StoreError> {
    let changed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_occurrence_settlement_slice s JOIN fact f ON f.uid = s.application_fact_uid WHERE s.occurrence_uid = ? AND s.owner_person_uid = ? AND (s.local_record_uid != ? OR json_extract(f.payload, '$.local_unit_uid') IS NOT (SELECT unit_uid FROM record WHERE uid = ?)) UNION ALL SELECT 1 FROM transfer_local_application a JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid JOIN fact f ON f.uid = a.application_fact_uid WHERE h.occurrence_uid = ? AND h.participant_person_uid = ? AND (a.local_record_uid != ? OR json_extract(f.payload, '$.local_unit_uid') IS NOT (SELECT unit_uid FROM record WHERE uid = ?)))")
        .bind(occurrence).bind(person).bind(record).bind(record).bind(occurrence).bind(person).bind(record).bind(record).fetch_one(&mut *connection).await?;
    if changed {
        return Err(invalid(
            "a partial application must retain its original private Record and unit",
        ));
    }
    Ok(())
}
