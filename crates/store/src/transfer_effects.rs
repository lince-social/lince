use chrono::{DateTime, Utc};
use nucleus::{DecimalValue, Fact, transfer::application::EffectMode};
use serde::Serialize;
use serde_json::json;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::{StoreError, exact, transfer_accounting as accounting};

fn invalid(value: impl ToString) -> StoreError {
    StoreError::Protocol(value.to_string())
}

fn mode(value: EffectMode) -> &'static str {
    match value {
        EffectMode::Quantity => "quantity",
        EffectMode::Fulfilment => "fulfilment",
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Effect {
    pub record: String,
    pub title: String,
    pub mode: EffectMode,
    pub unit: Option<String>,
    pub formula: String,
    pub before: DecimalValue,
    pub available_before: DecimalValue,
    pub delta: DecimalValue,
    pub after: DecimalValue,
    pub cumulative_before: DecimalValue,
    pub cumulative_after: DecimalValue,
}

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub transfer: String,
    pub exchange: String,
    pub effects: Vec<Effect>,
    pub hash: String,
}

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

pub async fn quote_on(
    connection: &mut SqliteConnection,
    binding: &accounting::Binding<'_>,
    canonical_after: DecimalValue,
) -> Result<Option<Group>, StoreError> {
    let application = accounting::effective_on(connection, binding).await?;
    let Some(policy) = &application.policy else {
        return Ok(None);
    };
    let previous = sqlx::query_as::<_, (String, String, Option<String>)>("SELECT DISTINCT record_uid,mode,unit_uid FROM transfer_private_effect WHERE transfer_uid = ? AND exchange_uid = ? AND (? IS NULL OR occurrence_uid = ?) AND person_uid = ? ORDER BY record_uid")
        .bind(binding.transfer).bind(binding.exchange).bind(binding.occurrence).bind(binding.occurrence).bind(binding.person).fetch_all(&mut *connection).await?;
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
    let mut effects = Vec::new();
    let availability = crate::transfer_loans::adjustments_on(
        connection,
        nucleus::execution::now().timestamp_millis(),
    )
    .await?;
    for (index, configured) in policy.effects.iter().enumerate() {
        let row = sqlx::query("SELECT r.head,r.quantity_mantissa,r.quantity_scale,r.unit_uid FROM record r JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ' AND own.kind = 'organ' AND own.deleted_at IS NULL WHERE r.uid = ? AND r.deleted_at IS NULL")
            .bind(&configured.record).fetch_optional(&mut *connection).await?
            .ok_or_else(|| invalid("every private effect needs a live Record on this Organ"))?;
        if availability
            .iter()
            .any(|adjustment| adjustment.record == configured.record && adjustment.unit_changed)
        {
            return Err(invalid("loan availability needs the original Record unit"));
        }
        let before = exact::read_decimal(&row, "quantity")?;
        let available_before = exact::sum_exact([
            before,
            availability
                .iter()
                .find(|adjustment| adjustment.record == configured.record)
                .map_or_else(exact::zero, |adjustment| adjustment.delta),
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
        formula = nucleus::transfer::application::validate(&formula).map_err(invalid)?;
        let rows = sqlx::query("SELECT f.delta_mantissa,f.delta_scale FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid WHERE e.transfer_uid = ? AND e.exchange_uid = ? AND (? IS NULL OR e.occurrence_uid = ?) AND e.person_uid = ? AND e.record_uid = ?")
            .bind(binding.transfer).bind(binding.exchange).bind(binding.occurrence).bind(binding.occurrence).bind(binding.person).bind(&configured.record).fetch_all(&mut *connection).await?;
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
        if nucleus::execution::current().is_some() && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'simulation_assumed_transfer_effect')").fetch_one(&mut *connection).await? {
            let assumed = sqlx::query("SELECT local_mantissa,local_scale FROM simulation_assumed_transfer_effect WHERE transfer_uid = ? AND exchange_uid = ? AND person_uid = ? AND record_uid = ? AND (? IS NULL OR occurrence_uid IS ? OR occurrence_uid IS NULL)")
                .bind(binding.transfer).bind(binding.exchange).bind(binding.person).bind(&configured.record).bind(binding.occurrence).bind(binding.occurrence).fetch_all(&mut *connection).await?;
            for row in assumed { cumulative_before = exact::sum_exact([cumulative_before, exact::read_decimal(&row, "local")?])?; }
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
    let hash = nucleus::fact::sha256_hex(
        serde_json::to_vec(
            &json!({"policy":policy.hash,"canonical_after":canonical_after,"effects":effects}),
        )
        .map_err(invalid)?
        .as_slice(),
    );
    Ok(Some(Group {
        transfer: binding.transfer.into(),
        exchange: binding.exchange.into(),
        effects,
        hash,
    }))
}

pub fn require_review(group: Option<&Group>, reviewed: Option<&str>) -> Result<(), StoreError> {
    if group.map(|group| group.hash.as_str()) != reviewed {
        return Err(invalid("the private effect group changed after review"));
    }
    Ok(())
}

pub async fn reviewed_hash(pool: &SqlitePool, primary: &str) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT review_hash FROM transfer_private_effect WHERE fact_uid = ? AND primary_fact_uid = ?")
        .bind(primary).bind(primary).fetch_optional(pool).await
}

pub(crate) async fn apply_on<F>(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    group: Option<&Group>,
    primary: &Fact,
    occurrence: &str,
    person: &str,
    intent: Option<&str>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<(), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let Some(group) = group else { return Ok(()) };
    if group.effects.first().is_none_or(|effect| {
        effect.record != primary.record_uid
            || !effect.delta.exact_numeric_cmp(primary.delta).is_eq()
    }) {
        return Err(invalid(
            "the primary application does not match the reviewed group",
        ));
    }
    for (index, effect) in group.effects.iter().enumerate() {
        let fact = if index == 0 {
            primary.clone()
        } else {
            let mut fact = nucleus::fact::seal(nucleus::fact::NewFact {
                uid:None, record_uid:effect.record.clone(), delta:effect.delta, at:None, actor_uid:Some(person.into()),
                cause:primary.cause.clone(), payload:Some(json!({"action":"apply-transfer-private-effect", "primary_fact":primary.uid,
                    "occurrence":occurrence,"mode":effect.mode,"local_unit_uid":effect.unit,"review_hash":group.hash}).to_string()),
            }, &crate::facts::last_hash(connection).await?, now);
            fact.signature = sign(&fact.hash);
            if fact.signature.is_none() && intent.is_none() {
                return Err(invalid("a private effect needs its owner's signature"));
            }
            crate::facts::insert(connection, &fact).await?;
            if let Some(intent) = intent {
                crate::action_intents::link_pending_fact(connection, intent, &fact).await?;
            }
            fact
        };
        sqlx::query("INSERT INTO transfer_private_effect (fact_uid,primary_fact_uid,occurrence_uid,transfer_uid,exchange_uid,person_uid,record_uid,mode,unit_uid,formula,review_hash) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&fact.uid).bind(&primary.uid).bind(occurrence).bind(&group.transfer).bind(&group.exchange).bind(person).bind(&effect.record).bind(mode(effect.mode))
            .bind(&effect.unit).bind(&effect.formula).bind(&group.hash).execute(&mut **connection).await?;
    }
    for effect in group.effects.iter().skip(1) {
        crate::records::bump_quantity(connection, &effect.record, effect.delta, &now.to_rfc3339())
            .await?;
    }
    Ok(())
}

pub async fn facts(pool: &SqlitePool, primary: &str) -> Result<Vec<Fact>, StoreError> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT e.fact_uid FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid WHERE e.primary_fact_uid = ? AND e.fact_uid != ? ORDER BY f.rowid")
        .bind(primary).bind(primary).fetch_all(pool).await?;
    let mut facts = Vec::new();
    for uid in ids {
        facts.push(
            crate::facts::get(pool, &uid)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
    }
    Ok(facts)
}

pub async fn records(pool: &SqlitePool, primary: &str) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar("SELECT record_uid FROM fact WHERE uid = ? UNION SELECT record_uid FROM transfer_private_effect WHERE primary_fact_uid = ?")
        .bind(primary).bind(primary).fetch_all(pool).await
}

pub async fn recorded(
    pool: &SqlitePool,
    primary: &str,
) -> Result<Vec<serde_json::Value>, StoreError> {
    let rows = sqlx::query("SELECT e.*,f.delta_mantissa,f.delta_scale FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid WHERE e.primary_fact_uid = ? ORDER BY f.rowid")
        .bind(primary).fetch_all(pool).await?;
    rows.into_iter()
        .map(|row| {
            Ok(json!({
                "fact":row.get::<String,_>("fact_uid"),"record":row.get::<String,_>("record_uid"),
                "mode":row.get::<String,_>("mode"),"unit":row.get::<Option<String>,_>("unit_uid"),
                "formula":row.get::<String,_>("formula"),"delta":exact::read_decimal(&row,"delta")?,
            }))
        })
        .collect()
}

pub(crate) async fn compensate_on<F>(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    primary: &str,
    person: &str,
    intent: Option<&str>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<(), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let rows = sqlx::query("SELECT e.*,f.delta_mantissa,f.delta_scale,r.unit_uid AS current_unit,r.deleted_at,r.organ_uid,(SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL) AS own_organ FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid LEFT JOIN record r ON r.uid = e.record_uid WHERE e.primary_fact_uid = ? AND e.fact_uid != ? ORDER BY f.rowid")
        .bind(primary).bind(primary).fetch_all(&mut **tx).await?;
    for row in rows {
        if row.get::<String, _>("person_uid") != person
            || row.get::<Option<String>, _>("deleted_at").is_some()
            || row.get::<Option<String>, _>("organ_uid").is_none()
            || row.get::<Option<String>, _>("organ_uid")
                != row.get::<Option<String>, _>("own_organ")
            || row.get::<Option<String>, _>("unit_uid")
                != row.get::<Option<String>, _>("current_unit")
        {
            return Err(invalid(
                "restore every original private Record and unit before correcting the group",
            ));
        }
        let original: String = row.get("fact_uid");
        let record: String = row.get("record_uid");
        let delta = exact::negate(exact::read_decimal(&row, "delta")?)?;
        let mut fact = nucleus::fact::seal(nucleus::fact::NewFact {
            uid:None, record_uid:record.clone(), delta, at:None, actor_uid:Some(person.into()),
            cause:nucleus::Cause { kind:nucleus::CauseKind::Compensation, uid:Some(original.clone()) },
            payload:Some(json!({"action":"compensate-transfer-private-effect", "primary_fact":primary, "original_fact":original}).to_string()),
        }, &crate::facts::last_hash(tx).await?, now);
        fact.signature = sign(&fact.hash);
        if fact.signature.is_none() && intent.is_none() {
            return Err(invalid("a private correction needs the owner's signature"));
        }
        crate::facts::insert(tx, &fact).await?;
        if let Some(intent) = intent {
            crate::action_intents::link_pending_fact(tx, intent, &fact).await?;
        }
        crate::records::bump_quantity(tx, &record, delta, &now.to_rfc3339()).await?;
    }
    Ok(())
}

pub async fn correction_facts(pool: &SqlitePool, primary: &str) -> Result<Vec<Fact>, StoreError> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT f.uid FROM transfer_private_effect e JOIN fact f ON f.cause_uid = e.fact_uid AND f.cause_kind = 'compensation' WHERE e.primary_fact_uid = ? AND e.fact_uid != ? ORDER BY f.rowid")
        .bind(primary).bind(primary).fetch_all(pool).await?;
    let mut facts = Vec::new();
    for uid in ids {
        facts.push(
            crate::facts::get(pool, &uid)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
    }
    Ok(facts)
}

pub async fn protected(pool: &SqlitePool, fact: &str) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_private_effect e WHERE e.fact_uid = ? UNION ALL SELECT 1 FROM fact f JOIN transfer_private_effect e ON e.fact_uid = f.cause_uid WHERE f.uid = ? AND f.cause_kind = 'compensation' UNION ALL SELECT 1 FROM transfer_local_application WHERE application_fact_uid = ? UNION ALL SELECT 1 FROM transfer_private_application_correction WHERE fact_uid = ?)")
        .bind(fact).bind(fact).bind(fact).bind(fact).fetch_one(pool).await
}
