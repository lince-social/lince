use std::collections::{BTreeMap, BTreeSet};

use nucleus::DecimalValue;
use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::{StoreError, exact, transfer_accounting as accounting};

#[derive(Clone, Debug, Serialize)]
pub struct Commitment {
    pub transfer: String,
    pub title: String,
    pub revision: u64,
    pub promise: String,
    pub exchange: String,
    pub occurrence: Option<String>,
    pub person: String,
    pub record: String,
    pub application_record: String,
    pub unit: Option<String>,
    pub canonical_remaining: DecimalValue,
    pub private_remaining: DecimalValue,
    pub outgoing: bool,
    pub reserved: DecimalValue,
    pub state: String,
    pub due: Option<String>,
    pub alternative_group: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Balance {
    pub actual: DecimalValue,
    pub available: DecimalValue,
    pub reserved: DecimalValue,
    pub surplus: DecimalValue,
    pub can_offer: DecimalValue,
    pub commitments: Vec<Commitment>,
    pub incomplete: Vec<String>,
}

#[derive(Clone)]
struct Promise {
    transfer: String,
    title: String,
    revision: u64,
    uid: String,
    exchange: String,
    owner: String,
    record: Option<String>,
    quantity: f64,
    unit: Option<String>,
    state: String,
    reserve_from: String,
    giver: Option<String>,
    receiver: Option<String>,
    due: Option<String>,
    alternative: Option<String>,
    remote_occurrences: Option<Vec<Value>>,
    unavailable: bool,
}

impl Promise {
    fn outgoing(&self, person: &str) -> bool {
        self.giver
            .as_deref()
            .map_or(self.quantity < 0.0, |giver| giver == person)
    }

    fn reserves(&self) -> bool {
        match self.reserve_from.as_str() {
            "proposed" => matches!(
                self.state.as_str(),
                "proposed" | "agreed" | "active" | "kept"
            ),
            "agreed" => matches!(self.state.as_str(), "agreed" | "active" | "kept"),
            "active" => matches!(self.state.as_str(), "active" | "kept"),
            _ => false,
        }
    }
}

fn field(value: &Value, name: &str) -> Option<String> {
    value[name].as_str().map(str::to_string)
}

async fn promises(connection: &mut SqliteConnection) -> Result<Vec<Promise>, StoreError> {
    let rows = sqlx::query("SELECT p.*, COALESCE(t.revision, 0) AS transfer_revision, t.source_uid AS alternative_source, t.satiation, tr.head AS transfer_head FROM promise p LEFT JOIN transfer t ON t.record_uid = p.transfer_uid LEFT JOIN record tr ON tr.uid = t.record_uid WHERE NOT EXISTS(SELECT 1 FROM transfer_source_group_result done WHERE done.source_uid = t.source_uid AND done.policy = t.satiation AND done.transfer_uid != t.record_uid AND p.state NOT IN ('active','kept'))")
        .fetch_all(&mut *connection).await?;
    let mut promises = Vec::new();
    for row in rows {
        let item = row
            .get::<Option<String>, _>("item_json")
            .and_then(|value| serde_json::from_str::<Value>(&value).ok())
            .unwrap_or(Value::Null);
        let uid: String = row.get("uid");
        let transfer: Option<String> = row.get("transfer_uid");
        promises.push(Promise {
            transfer: transfer.unwrap_or_default(),
            title: field(&item, "title")
                .or(row.get::<Option<String>, _>("transfer_head"))
                .unwrap_or_else(|| "Commitment".into()),
            revision: row.get::<i64, _>("transfer_revision") as u64,
            exchange: field(&item["exchange"], "uid").unwrap_or_else(|| uid.clone()),
            uid,
            owner: row
                .get::<Option<String>, _>("party_uid")
                .unwrap_or_default(),
            record: row.get("record_uid"),
            quantity: row.get("delta"),
            unit: row.get("unit_uid"),
            state: row.get("state"),
            reserve_from: row.get("reserve_from"),
            giver: field(&item["exchange"], "giver"),
            receiver: field(&item["exchange"], "receiver"),
            due: row.get("window_end"),
            alternative: if row.get::<Option<String>, _>("satiation").as_deref()
                == Some("first_completes")
            {
                row.get("alternative_source")
            } else {
                None
            },
            remote_occurrences: None,
            unavailable: false,
        });
    }
    let references: Vec<(String, String)> = sqlx::query_as("SELECT projection, state FROM transfer_remote_reference WHERE projection IS NOT NULL AND recipient_organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ') ORDER BY last_transfer_revision")
        .fetch_all(&mut *connection).await?;
    for (projection, state) in references {
        let projection: Value = serde_json::from_str(&projection)
            .map_err(|error| StoreError::Protocol(error.to_string()))?;
        let transfer = field(&projection, "uid").unwrap_or_default();
        let revision = projection["revision"].as_u64().unwrap_or(0);
        for row in projection["promises"].as_array().into_iter().flatten() {
            if projection["status"] == "satiated"
                && !matches!(row["state"].as_str(), Some("active" | "kept"))
            {
                continue;
            }

            let Some(uid) = field(row, "uid") else {
                continue;
            };
            if promises.iter().any(|promise| {
                promise.transfer == transfer && promise.uid == uid && promise.revision >= revision
            }) {
                continue;
            }
            promises.retain(|promise| promise.transfer != transfer || promise.uid != uid);
            promises.push(Promise {
                transfer: transfer.clone(),
                title: field(row, "title")
                    .or_else(|| field(&row["item"], "title"))
                    .or_else(|| field(&projection, "head"))
                    .unwrap_or_else(|| "Commitment".into()),
                revision,
                exchange: field(row, "exchange").unwrap_or_else(|| uid.clone()),
                uid,
                owner: field(row, "party").unwrap_or_default(),
                record: field(row, "record"),
                quantity: row["delta"].as_f64().unwrap_or(0.0),
                unit: field(row, "unit"),
                state: field(row, "state").unwrap_or_default(),
                reserve_from: field(row, "reserve_from").unwrap_or_else(|| "none".into()),
                giver: field(row, "giver"),
                receiver: field(row, "receiver"),
                due: field(row, "window_end"),
                alternative: if projection["satiation"] == "first_completes" {
                    field(&projection, "source")
                } else {
                    None
                },
                remote_occurrences: Some(
                    projection["occurrences"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                ),
                unavailable: state != "active",
            });
        }
    }
    Ok(promises)
}

pub async fn read(pool: &SqlitePool, record: &str) -> Result<Balance, StoreError> {
    let mut tx = pool.begin().await?;
    let balance = read_on(&mut tx, record).await?;
    tx.rollback().await?;
    Ok(balance)
}

pub async fn read_on(
    connection: &mut SqliteConnection,
    record: &str,
) -> Result<Balance, StoreError> {
    let resource = sqlx::query(
        "SELECT quantity_mantissa, quantity_scale FROM record WHERE uid = ? AND deleted_at IS NULL",
    )
    .bind(record)
    .fetch_one(&mut *connection)
    .await?;
    let actual = exact::read_decimal(&resource, "quantity")?;
    let mappings: Vec<(String,String,String,String)> = sqlx::query_as("SELECT e.transfer_uid, e.exchange_uid, e.person_uid, COALESCE(json_extract(effect.value, '$.record'),e.record_uid) FROM transfer_private_policy p JOIN transfer_private_policy_event e ON e.uid = p.event_uid LEFT JOIN json_each(e.effects_json) effect")
        .fetch_all(&mut *connection).await?;
    let previous: Vec<(String,String,String,String)> = sqlx::query_as("SELECT DISTINCT h.transfer_uid, h.source_promise_uid, h.participant_person_uid, a.local_record_uid FROM transfer_local_application a JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid WHERE a.local_record_uid = ?")
        .bind(record).fetch_all(&mut *connection).await?;
    let all = promises(connection).await?;
    let mut selected: BTreeMap<(String, String, String), Promise> = BTreeMap::new();
    for promise in all {
        let mut people = BTreeSet::new();
        if promise.record.as_deref() == Some(record) {
            people.insert(promise.owner.clone());
        }
        for (transfer, exchange, person, target) in &mappings {
            if *transfer == promise.transfer && *exchange == promise.exchange && target == record {
                people.insert(person.clone());
            }
        }
        for (transfer, source, person, _) in &previous {
            if *transfer == promise.transfer && *source == promise.uid {
                people.insert(person.clone());
            }
        }
        for person in people {
            if person != promise.owner
                && promise.giver.as_deref() != Some(&person)
                && promise.receiver.as_deref() != Some(&person)
            {
                continue;
            }
            if mappings.iter().any(|(transfer, exchange, owner, _)| {
                *transfer == promise.transfer && *exchange == promise.exchange && *owner == person
            }) && !mappings.iter().any(|(transfer, exchange, owner, target)| {
                *transfer == promise.transfer
                    && *exchange == promise.exchange
                    && *owner == person
                    && target == record
            }) {
                continue;
            }
            let key = (
                promise.transfer.clone(),
                promise.exchange.clone(),
                person.clone(),
            );
            let replace = selected.get(&key).is_none_or(|current| {
                promise.revision > current.revision
                    || (current.owner != person && promise.owner == person)
            });
            if replace {
                selected.insert(key, promise.clone());
            }
        }
    }
    let mut commitments = Vec::new();
    let mut incomplete = Vec::new();
    for (transfer, exchange, person, target) in &mappings {
        if target == record
            && !selected.contains_key(&(transfer.clone(), exchange.clone(), person.clone()))
        {
            incomplete.push(format!(
                "{transfer}: a private commitment has no available current terms"
            ));
        }
    }
    for ((_, _, person), promise) in selected {
        if matches!(
            promise.state.as_str(),
            "open" | "withdrawn" | "broken" | "expired"
        ) {
            continue;
        }
        if promise.unavailable {
            incomplete.push(format!(
                "{}: delivery access is unavailable",
                promise.transfer
            ));
        }
        let occurrences = if let Some(rows) = &promise.remote_occurrences {
            rows.clone()
        } else {
            sqlx::query("SELECT o.uid, o.promise_uid, o.quantity, o.window_end, o.exchange_path_uid FROM transfer_occurrence o JOIN transfer_exchange_path path ON path.uid = o.exchange_path_uid WHERE o.transfer_uid = ? AND COALESCE(path.public_exchange_uid,o.promise_uid) = ? ORDER BY o.created_at,o.uid")
                .bind(&promise.transfer).bind(&promise.exchange).fetch_all(&mut *connection).await?
                .into_iter().map(|row|serde_json::json!({"uid":row.get::<String,_>("uid"),"promise":row.get::<String,_>("promise_uid"),"quantity":row.get::<f64,_>("quantity"),"window_end":row.get::<Option<String>,_>("window_end"),"exchange":promise.exchange,"exchange_path":row.get::<String,_>("exchange_path_uid")})).collect()
        };
        let mut paths: BTreeMap<String, Value> = BTreeMap::new();
        for occurrence in occurrences
            .into_iter()
            .filter(|row| row["exchange"] == promise.exchange || row["promise"] == promise.uid)
        {
            let Some(uid) = field(&occurrence, "uid") else {
                continue;
            };
            let path = field(&occurrence, "exchange_path").unwrap_or(uid);
            if !paths.contains_key(&path) || occurrence["promise"] == promise.uid {
                paths.insert(path, occurrence);
            }
        }
        if paths.is_empty() && promise.state == "kept" {
            continue;
        }
        let amounts: Vec<(Option<String>, f64, Option<String>)> = if paths.is_empty() {
            vec![(None, promise.quantity.abs(), promise.due.clone())]
        } else {
            paths
                .values()
                .map(|row| {
                    (
                        field(row, "uid"),
                        row["quantity"].as_f64().unwrap_or(0.0),
                        field(row, "window_end"),
                    )
                })
                .collect()
        };
        for (occurrence, total, due) in amounts {
            let result = commitment(
                connection,
                record,
                &person,
                &promise,
                occurrence.as_deref(),
                total,
                due,
            )
            .await;
            match result {
                Ok(Some(commitment)) => commitments.push(commitment),
                Ok(None) => {}
                Err(error) => incomplete.push(format!("{}: {error}", promise.transfer)),
            }
        }
    }
    let mut reservations = Vec::new();
    let mut alternatives: BTreeMap<(String, String), BTreeMap<String, (DecimalValue, bool)>> =
        BTreeMap::new();
    for commitment in &commitments {
        if let Some(group) = &commitment.alternative_group {
            let (amount, active) = alternatives
                .entry((group.clone(), commitment.person.clone()))
                .or_default()
                .entry(commitment.transfer.clone())
                .or_insert((exact::zero(), false));
            *amount = exact::sum_exact([*amount, commitment.reserved])?;
            *active |= matches!(commitment.state.as_str(), "active" | "kept");
        } else {
            reservations.push(commitment.reserved);
        }
    }
    for choices in alternatives.into_values() {
        if choices.values().any(|(_, active)| *active) {
            reservations.extend(
                choices
                    .into_values()
                    .filter(|(_, active)| *active)
                    .map(|(amount, _)| amount),
            );
        } else {
            reservations.push(
                choices
                    .into_values()
                    .map(|(amount, _)| amount)
                    .max_by(|left, right| left.exact_numeric_cmp(*right))
                    .unwrap_or_else(exact::zero),
            );
        }
    }
    let reserved = exact::sum_exact(reservations)?;
    let availability = crate::transfer_loans::adjustments_on(
        connection,
        nucleus::execution::now().timestamp_millis(),
    )
    .await?;
    let loan_unit_changed = availability
        .iter()
        .any(|adjustment| adjustment.record == record && adjustment.unit_changed);
    if loan_unit_changed {
        incomplete.push("Loan availability needs the original Record unit".into());
    }
    let available = exact::sum_exact([
        actual,
        availability
            .iter()
            .find(|adjustment| adjustment.record == record)
            .map_or_else(exact::zero, |adjustment| adjustment.delta),
    ])?;
    let surplus = exact::difference(available, reserved)?;
    let can_offer = if loan_unit_changed || surplus.is_negative() {
        exact::zero()
    } else {
        surplus
    };
    Ok(Balance {
        actual,
        available,
        reserved,
        surplus,
        can_offer,
        commitments,
        incomplete,
    })
}

async fn commitment(
    connection: &mut SqliteConnection,
    record: &str,
    person: &str,
    promise: &Promise,
    occurrence: Option<&str>,
    total: f64,
    due: Option<String>,
) -> Result<Option<Commitment>, StoreError> {
    if total <= 0.0 {
        return Err(StoreError::Protocol(
            "the agreed quantity is unavailable".into(),
        ));
    }
    let mut applied = match occurrence {
        Some(occurrence) => accounting::applied_on(connection, occurrence, person).await?,
        None => accounting::Applied {
            canonical: exact::zero(),
            local: exact::zero(),
        },
    };
    let scenario_table = if nucleus::execution::current().is_some() {
        sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'simulation_assumed_transfer_effect')").fetch_one(&mut *connection).await?
    } else {
        false
    };
    if scenario_table && nucleus::execution::current().is_some() {
        let effects = sqlx::query("SELECT canonical_mantissa, canonical_scale, local_mantissa, local_scale FROM simulation_assumed_transfer_effect WHERE transfer_uid = ? AND exchange_uid = ? AND person_uid = ? AND record_uid = ? AND (occurrence_uid IS ? OR occurrence_uid IS NULL)")
            .bind(&promise.transfer).bind(&promise.exchange).bind(person).bind(record).bind(occurrence).fetch_all(&mut *connection).await?;
        for effect in effects {
            applied.canonical = exact::sum_exact([
                applied.canonical,
                exact::read_decimal(&effect, "canonical")?,
            ])?;
            applied.local =
                exact::sum_exact([applied.local, exact::read_decimal(&effect, "local")?])?;
        }
    }
    let total = accounting::amount(total)?;
    let cancelled = if let Some(occurrence) = occurrence {
        if let Some(remote) = &promise.remote_occurrences {
            let row = remote.iter().find(|row| row["uid"] == occurrence);
            match row
                .and_then(|row| row["settlement_progress"]["cancelled_quantity_exact"].as_str())
            {
                Some(value) => DecimalValue::parse_inferred(value)
                    .map_err(|error| StoreError::Protocol(error.to_string()))?,
                None => accounting::amount(
                    row.and_then(|row| row["settlement_progress"]["cancelled_quantity"].as_f64())
                        .unwrap_or(0.0),
                )?,
            }
        } else {
            crate::transfer_cancellations::cancelled_on(connection, occurrence).await?
        }
    } else {
        exact::zero()
    };
    let total = exact::difference(total, cancelled)?;
    let remaining = exact::difference(total, applied.canonical)?;
    if !remaining.is_positive() {
        return Ok(None);
    }
    let policy_record =
        accounting::policy_on(connection, &promise.transfer, &promise.exchange, person)
            .await?
            .map(|policy| policy.record)
            .unwrap_or_else(|| record.into());
    let binding = accounting::Binding {
        transfer: &promise.transfer,
        exchange: &promise.exchange,
        occurrence,
        person,
        record: Some(&policy_record),
        unit: promise.unit.as_deref(),
        outgoing: promise.outgoing(person),
    };
    let policy = accounting::effective_on(connection, &binding).await?;
    let private_remaining = if let Some(group) =
        crate::transfer_effects::quote_on(connection, &binding, total).await?
    {
        group
            .effects
            .iter()
            .find(|effect| effect.record == record)
            .ok_or_else(|| {
                StoreError::Protocol("private Record is outside the effect group".into())
            })?
            .delta
    } else {
        accounting::calculate(&policy.formula, total, applied.local)?.0
    };
    let reserved = if promise.reserves() && private_remaining.is_negative() {
        exact::negate(private_remaining)?
    } else {
        exact::zero()
    };
    Ok(Some(Commitment {
        transfer: promise.transfer.clone(),
        title: promise.title.clone(),
        revision: promise.revision,
        promise: promise.uid.clone(),
        exchange: promise.exchange.clone(),
        occurrence: occurrence.map(str::to_string),
        person: person.into(),
        record: record.into(),
        application_record: policy_record,
        unit: promise.unit.clone(),
        canonical_remaining: remaining,
        private_remaining,
        outgoing: promise.outgoing(person),
        reserved,
        state: promise.state.clone(),
        due,
        alternative_group: promise.alternative.clone(),
    }))
}
