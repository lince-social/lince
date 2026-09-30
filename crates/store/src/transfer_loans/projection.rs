use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, transfer::disclosure::TransferItem};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{Accepted, invalid};
use crate::{StoreError, exact};

#[derive(Clone, Debug, Serialize)]
pub struct Terms {
    pub origin: String,
    pub transfer: String,
    pub item: TransferItem,
    pub accepted: Option<Accepted>,
    pub settled: DecimalValue,
    pub retained: bool,
}

fn positive(value: DecimalValue) -> DecimalValue {
    if value.is_negative() {
        exact::zero()
    } else {
        value
    }
}

fn minimum(a: DecimalValue, b: DecimalValue) -> DecimalValue {
    if a.exact_numeric_cmp(b).is_gt() { b } else { a }
}

pub async fn terms(pool: &SqlitePool, retained: bool) -> Result<Vec<Terms>, StoreError> {
    terms_on(&mut *pool.acquire().await?, retained).await
}

async fn terms_on(
    connection: &mut SqliteConnection,
    retained: bool,
) -> Result<Vec<Terms>, StoreError> {
    let mut result = BTreeMap::new();
    let rows = sqlx::query("SELECT p.uid,p.transfer_uid,p.item_json,r.organ_uid FROM promise p JOIN transfer t ON t.record_uid = p.transfer_uid JOIN record r ON r.uid = t.record_uid JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ' WHERE r.deleted_at IS NULL AND p.item_json IS NOT NULL AND (json_extract(p.item_json,'$.loan') IS NOT NULL OR json_extract(p.item_json,'$.return_of') IS NOT NULL) ORDER BY p.uid")
        .fetch_all(&mut *connection).await?;
    for row in rows {
        let item: TransferItem = serde_json::from_str(row.get("item_json")).map_err(invalid)?;
        let Some(exchange) = item.exchange.as_ref() else {
            continue;
        };
        let transfer: String = row.get("transfer_uid");
        let origin: String = row.get("organ_uid");
        let settled: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(s.canonical_quantity),0.0) FROM transfer_occurrence_settlement_slice s JOIN transfer_occurrence o ON o.uid = s.occurrence_uid WHERE s.promise_uid = ? AND o.disputed = 0")
            .bind(row.get::<String,_>("uid")).fetch_one(&mut *connection).await?;
        let settled = nucleus::transfer::application::amount(settled).map_err(invalid)?;
        let key = (origin.clone(), transfer.clone(), exchange.uid.clone());
        if result
            .get(&key)
            .is_some_and(|old: &Terms| old.settled.exact_numeric_cmp(settled).is_ge())
        {
            continue;
        }
        let accepted = super::accepted_on(connection, &transfer)
            .await?
            .into_iter()
            .find(|loan| loan.exchange == exchange.uid);
        result.insert(
            key,
            Terms {
                origin,
                transfer,
                item,
                accepted,
                settled,
                retained: false,
            },
        );
    }
    let rows = sqlx::query("SELECT r.origin_organ_uid,r.transfer_uid,r.state,r.projection FROM transfer_remote_reference r JOIN record own ON own.uid = r.recipient_organ_uid AND own.slug = 'local-organ' WHERE r.projection IS NOT NULL AND (? OR r.state = 'active') ORDER BY r.last_transfer_revision DESC,r.last_cursor DESC,r.uid")
        .bind(retained).fetch_all(&mut *connection).await?;
    let mut seen = BTreeSet::new();
    for row in rows {
        let origin: String = row.get("origin_organ_uid");
        let transfer: String = row.get("transfer_uid");
        let view: Value = serde_json::from_str(row.get("projection")).map_err(invalid)?;
        for promise in view["promises"].as_array().into_iter().flatten() {
            let Some(exchange) = promise["exchange"].as_str() else {
                continue;
            };
            let key = (origin.clone(), transfer.clone(), exchange.to_owned());
            if result.contains_key(&key) || !seen.insert(key.clone()) {
                continue;
            }
            let Some(giver) = promise["giver"].as_str() else {
                continue;
            };
            let Some(receiver) = promise["receiver"].as_str() else {
                continue;
            };
            let item = TransferItem {
                title: promise["title"].as_str().unwrap_or("Loan").to_owned(),
                exchange: Some(nucleus::transfer::exchange::ExchangeRoute {
                    uid: exchange.into(),
                    giver: giver.into(),
                    receiver: receiver.into(),
                }),
                loan: serde_json::from_value(promise["loan"].clone()).map_err(invalid)?,
                return_of: serde_json::from_value(promise["return_of"].clone()).map_err(invalid)?,
                ..Default::default()
            };
            if item.loan.is_none() && item.return_of.is_none() {
                continue;
            }
            let accepted =
                serde_json::from_value(promise["accepted_loan"].clone()).map_err(invalid)?;
            let mut per_promise = BTreeMap::<String, DecimalValue>::new();
            for occurrence in view["occurrences"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|occurrence| occurrence["disputed"] != true)
            {
                let Some(uid) = occurrence["promise"].as_str() else {
                    continue;
                };
                if !view["promises"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|p| p["uid"] == uid && p["exchange"] == exchange)
                {
                    continue;
                }
                let quantity = nucleus::transfer::application::amount(
                    occurrence["settlement_progress"]["settled_quantity"]
                        .as_f64()
                        .unwrap_or(0.0),
                )
                .map_err(invalid)?;
                let total = per_promise.entry(uid.into()).or_insert_with(exact::zero);
                *total = exact::sum_exact([*total, quantity])?;
            }
            let settled = per_promise
                .into_values()
                .max_by(|a, b| a.exact_numeric_cmp(*b))
                .unwrap_or_else(exact::zero);
            result.insert(
                key,
                Terms {
                    origin: origin.clone(),
                    transfer: transfer.clone(),
                    item,
                    accepted,
                    settled,
                    retained: row.get::<String, _>("state") != "active",
                },
            );
        }
    }
    Ok(result.into_values().collect())
}

pub async fn validate_link(
    pool: &SqlitePool,
    item: &TransferItem,
    person: &str,
) -> Result<(), StoreError> {
    let Some(reference) = item.return_of.as_ref().or(item.future_need_for.as_ref()) else {
        return Ok(());
    };
    let authorized: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record own WHERE own.slug = 'local-organ' AND own.uid = ?) OR EXISTS(SELECT 1 FROM transfer_remote_reference WHERE origin_organ_uid = ? AND transfer_uid = ? AND recipient_person_uid = ? AND state = 'active' AND projection IS NOT NULL)")
        .bind(&reference.origin).bind(&reference.origin).bind(&reference.transfer).bind(person).fetch_one(pool).await?;
    if !authorized {
        return Err(invalid("the linked loan is unavailable to this Person"));
    }
    let source = terms(pool, false)
        .await?
        .into_iter()
        .find(|source| {
            source.origin == reference.origin
                && source.transfer == reference.transfer
                && source
                    .item
                    .exchange
                    .as_ref()
                    .is_some_and(|route| route.uid == reference.exchange)
        })
        .ok_or_else(|| invalid("the linked loan is unavailable to this Person"))?;
    let route = source
        .item
        .exchange
        .as_ref()
        .ok_or_else(|| invalid("loan route unavailable"))?;
    if source.accepted.is_none() || (person != route.giver && person != route.receiver) {
        return Err(invalid(
            "a linked proposal needs an accepted loan and its lender or borrower",
        ));
    }
    if item.return_of.is_some()
        && !item.exchange.as_ref().is_some_and(|returned| {
            returned.giver == route.receiver && returned.receiver == route.giver
        })
    {
        return Err(invalid(
            "a return must go from the borrower to the original lender",
        ));
    }
    if item.future_need_for.is_some() && person != route.receiver {
        return Err(invalid("only the borrower can publish this future Need"));
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct Adjustment {
    pub record: String,
    pub delta: DecimalValue,
    pub next_ms: Option<i64>,
    pub unit_changed: bool,
}

struct Effect {
    transfer: String,
    exchange: String,
    person: String,
    record: String,
    delta: DecimalValue,
    fulfilment: bool,
    unit: Option<String>,
}

async fn effects(connection: &mut SqliteConnection) -> Result<Vec<Effect>, StoreError> {
    let rows = sqlx::query("SELECT e.transfer_uid,e.exchange_uid,e.person_uid,e.record_uid,e.mode,e.unit_uid,f.uid,f.delta_mantissa,f.delta_scale FROM transfer_private_effect e JOIN fact f ON f.uid = e.fact_uid UNION ALL SELECT s.transfer_uid,json_extract(p.item_json,'$.exchange.uid'),s.owner_person_uid,f.record_uid,'quantity',json_extract(f.payload,'$.local_unit_uid'),f.uid,f.delta_mantissa,f.delta_scale FROM transfer_occurrence_settlement_slice s JOIN promise p ON p.uid = s.promise_uid JOIN fact f ON f.uid = s.application_fact_uid WHERE NOT EXISTS(SELECT 1 FROM transfer_private_effect e WHERE e.fact_uid = f.uid) AND json_extract(p.item_json,'$.exchange.uid') IS NOT NULL UNION ALL SELECT h.transfer_uid,json_extract(p.value,'$.exchange'),a.participant_person_uid,a.local_record_uid,'quantity',json_extract(f.payload,'$.local_unit_uid'),f.uid,f.delta_mantissa,f.delta_scale FROM transfer_local_application a JOIN transfer_application_effect_handoff h ON h.uid = a.handoff_uid JOIN fact f ON f.uid = a.application_fact_uid JOIN transfer_remote_reference r ON r.uid = h.reference_uid,json_each(r.projection,'$.promises') p WHERE json_extract(p.value,'$.uid') = h.source_promise_uid AND json_extract(p.value,'$.exchange') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM transfer_private_effect e WHERE e.fact_uid = f.uid)")
        .fetch_all(&mut *connection).await?;
    let mut result = Vec::new();
    for row in rows {
        let corrections = sqlx::query("SELECT f.delta_mantissa,f.delta_scale FROM fact f WHERE (f.cause_kind = 'compensation' AND f.cause_uid = ?1) OR EXISTS (SELECT 1 FROM fact_origin o JOIN record own ON own.uid = o.organ_uid AND own.slug = 'local-organ' WHERE o.fact_uid = f.uid AND json_extract(o.payload, '$.cause.kind') = 'compensation' AND json_extract(o.payload, '$.cause.uid') = ?1)")
            .bind(row.get::<String,_>("uid")).fetch_all(&mut *connection).await?;
        let delta = exact::sum_exact(
            std::iter::once(exact::read_decimal(&row, "delta")?).chain(
                corrections
                    .iter()
                    .map(|row| exact::read_decimal(row, "delta"))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        )?;
        result.push(Effect {
            transfer: row.get("transfer_uid"),
            exchange: row.get("exchange_uid"),
            person: row.get("person_uid"),
            record: row.get("record_uid"),
            delta,
            fulfilment: row.get::<String, _>("mode") == "fulfilment",
            unit: row.get("unit_uid"),
        });
    }
    let assumed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'simulation_assumed_transfer_effect')").fetch_one(&mut *connection).await?;
    if assumed && nucleus::execution::current().is_some() {
        for row in sqlx::query("SELECT * FROM simulation_assumed_transfer_effect")
            .fetch_all(&mut *connection)
            .await?
        {
            result.push(Effect {
                transfer: row.get("transfer_uid"),
                exchange: row.get("exchange_uid"),
                person: row.get("person_uid"),
                record: row.get("record_uid"),
                delta: exact::read_decimal(&row, "local")?,
                fulfilment: row.get::<String, _>("mode") == "fulfilment",
                unit: row.get("unit_uid"),
            });
        }
    }
    Ok(result)
}

pub async fn adjustments(pool: &SqlitePool, at_ms: i64) -> Result<Vec<Adjustment>, StoreError> {
    adjustments_on(&mut *pool.acquire().await?, at_ms).await
}

pub async fn adjustments_on(
    connection: &mut SqliteConnection,
    at_ms: i64,
) -> Result<Vec<Adjustment>, StoreError> {
    let mut terms = terms_on(connection, true).await?;
    if !terms.iter().any(|terms| {
        terms.accepted.is_some()
            || nucleus::execution::current().is_some() && terms.item.loan.is_some()
    }) {
        return Ok(Vec::new());
    }
    let effects = effects(connection).await?;
    let has_assumptions: bool = nucleus::execution::current().is_some() && sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'simulation_loan_timing')").fetch_one(&mut *connection).await?;
    let has_receipts: bool = nucleus::execution::current().is_some() && sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'simulation_assumed_transfer_effect')").fetch_one(&mut *connection).await?;
    if has_receipts {
        let rows = sqlx::query("SELECT transfer_uid,exchange_uid,person_uid,occurrence_uid,cumulative_mantissa,cumulative_scale FROM simulation_assumed_transfer_effect").fetch_all(&mut *connection).await?;
        let mut occurrences = BTreeMap::new();
        for row in rows {
            let key = (
                row.get::<String, _>("transfer_uid"),
                row.get::<String, _>("exchange_uid"),
                row.get::<String, _>("person_uid"),
                row.get::<Option<String>, _>("occurrence_uid"),
            );
            let amount = exact::read_decimal(&row, "cumulative")?;
            let current = occurrences.entry(key).or_insert_with(exact::zero);
            if amount.exact_numeric_cmp(*current).is_gt() {
                *current = amount;
            }
        }
        let mut people = BTreeMap::new();
        for ((transfer, exchange, person, _), amount) in occurrences {
            let current = people
                .entry((transfer, exchange, person))
                .or_insert_with(exact::zero);
            *current = exact::sum_exact([*current, amount])?;
        }
        for source in &mut terms {
            for ((transfer, exchange, _), amount) in &people {
                if source.transfer == *transfer
                    && source
                        .item
                        .exchange
                        .as_ref()
                        .is_some_and(|route| route.uid == *exchange)
                    && amount.exact_numeric_cmp(source.settled).is_gt()
                {
                    source.settled = *amount;
                }
            }
        }
    }
    let mut result = BTreeMap::<String, Adjustment>::new();
    let mut active_fulfilment = BTreeMap::new();
    for source in &terms {
        let Some(route) = &source.item.exchange else {
            continue;
        };
        let mut loan = if let Some(loan) = &source.accepted {
            loan.clone()
        } else if has_receipts && let Some(interval) = &source.item.loan {
            let assumed:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM simulation_assumed_transfer_effect WHERE transfer_uid = ? AND exchange_uid = ? AND person_uid = ?)")
                .bind(&source.transfer).bind(&route.uid).bind(&route.receiver).fetch_one(&mut *connection).await?;
            if !assumed {
                continue;
            }
            let (from_ms, until_ms) = interval.bounds().map_err(invalid)?;
            Accepted {
                origin: source.origin.clone(),
                exchange: route.uid.clone(),
                revision: 0,
                from_ms,
                until_ms,
                agreement_fact: String::new(),
            }
        } else {
            continue;
        };
        let assumed_return = if has_assumptions {
            if let Some(row) = sqlx::query("SELECT * FROM simulation_loan_timing WHERE origin = ? AND transfer_uid = ? AND exchange_uid = ? AND person_uid = ?")
                .bind(&source.origin).bind(&source.transfer).bind(&route.uid).bind(&route.receiver).fetch_optional(&mut *connection).await? {
                loan.until_ms = row.get("until_ms");
                exact::read_decimal(&row,"returned")?
            } else { exact::zero() }
        } else {
            exact::zero()
        };
        let returns: Vec<_> = terms
            .iter()
            .filter(|returned| {
                returned.item.return_of.as_ref().is_some_and(|reference| {
                    reference.origin == source.origin
                        && reference.transfer == source.transfer
                        && reference.exchange == route.uid
                })
            })
            .collect();
        let returned = exact::sum_exact(returns.iter().map(|returned| returned.settled))?;
        let returned = if assumed_return.exact_numeric_cmp(returned).is_gt() {
            assumed_return
        } else {
            returned
        };
        let mut contributions = BTreeMap::new();
        for effect in effects.iter().filter(|effect| {
            effect.transfer == source.transfer
                && effect.exchange == route.uid
                && effect.person == route.receiver
        }) {
            let delta = contributions
                .entry(effect.record.clone())
                .or_insert_with(exact::zero);
            *delta = exact::sum_exact([*delta, effect.delta])?;
        }
        for (record, credit) in contributions {
            let credit = positive(credit);
            let returned_credit = if source.settled.is_zero() {
                exact::zero()
            } else {
                crate::transfer_accounting::calculate(
                    &format!(
                        "incoming() * {} / {}",
                        minimum(returned, source.settled),
                        source.settled
                    ),
                    credit,
                    exact::zero(),
                )?
                .0
            };
            let removed = if at_ms < loan.from_ms || at_ms >= loan.until_ms {
                credit
            } else {
                returned_credit
            };
            let physical_debit = positive(exact::negate(exact::sum_exact(
                effects
                    .iter()
                    .filter(|effect| {
                        effect.record == record
                            && effect.person == route.receiver
                            && returns.iter().any(|returned| {
                                returned.transfer == effect.transfer
                                    && returned
                                        .item
                                        .exchange
                                        .as_ref()
                                        .is_some_and(|route| route.uid == effect.exchange)
                            })
                    })
                    .map(|effect| effect.delta),
            )?)?);
            let units: BTreeSet<_> = effects
                .iter()
                .filter(|effect| {
                    effect.record == record
                        && effect.person == route.receiver
                        && !effect.delta.is_zero()
                        && (effect.transfer == source.transfer && effect.exchange == route.uid
                            || returns.iter().any(|returned| {
                                returned.transfer == effect.transfer
                                    && returned
                                        .item
                                        .exchange
                                        .as_ref()
                                        .is_some_and(|route| route.uid == effect.exchange)
                            }))
                })
                .map(|effect| effect.unit.clone())
                .collect();
            let unit: Option<String> =
                sqlx::query_scalar("SELECT unit_uid FROM record WHERE uid = ?")
                    .bind(&record)
                    .fetch_one(&mut *connection)
                    .await?;
            let unit_changed = !credit.is_zero()
                && (units.len() > 1
                    || credit.exact_numeric_cmp(physical_debit).is_gt()
                        && units.first() != Some(&unit));
            let delta = exact::negate(positive(exact::difference(removed, physical_debit)?))?;
            if effects.iter().any(|effect| {
                effect.transfer == source.transfer
                    && effect.exchange == route.uid
                    && effect.person == route.receiver
                    && effect.record == record
                    && effect.fulfilment
            }) {
                let active = positive(exact::difference(
                    credit,
                    if removed.exact_numeric_cmp(physical_debit).is_gt() {
                        removed
                    } else {
                        physical_debit
                    },
                )?);
                let total = active_fulfilment
                    .entry(record.clone())
                    .or_insert_with(exact::zero);
                *total = exact::sum_exact([*total, active])?;
            }
            let next_ms = [loan.from_ms, loan.until_ms]
                .into_iter()
                .filter(|time| *time > at_ms)
                .min();
            let adjustment = result.entry(record.clone()).or_insert(Adjustment {
                record,
                delta: exact::zero(),
                next_ms: None,
                unit_changed: false,
            });
            adjustment.unit_changed |= unit_changed;
            adjustment.delta = exact::sum_exact([adjustment.delta, delta])?;
            adjustment.next_ms = adjustment.next_ms.into_iter().chain(next_ms).min();
        }
    }
    for (record, credit) in active_fulfilment {
        let Some(adjustment) = result.get_mut(&record) else {
            continue;
        };
        if let Some(row) =
            sqlx::query("SELECT quantity_mantissa,quantity_scale FROM record WHERE uid = ?")
                .bind(record)
                .fetch_optional(&mut *connection)
                .await?
        {
            let available =
                exact::sum_exact([exact::read_decimal(&row, "quantity")?, adjustment.delta])?;
            adjustment.delta =
                exact::difference(adjustment.delta, minimum(credit, positive(available)))?;
        }
    }
    for adjustment in result.values_mut() {
        if adjustment.unit_changed {
            adjustment.delta = exact::zero();
        }
    }
    Ok(result.into_values().collect())
}

pub fn status(source: &Terms, all: &[Terms], at_ms: i64) -> Result<serde_json::Value, StoreError> {
    let (Some(loan), Some(route)) = (&source.accepted, &source.item.exchange) else {
        return Ok(Value::Null);
    };
    let returned = exact::sum_exact(
        all.iter()
            .filter(|term| {
                term.item.return_of.as_ref().is_some_and(|reference| {
                    reference.origin == source.origin
                        && reference.transfer == source.transfer
                        && reference.exchange == route.uid
                })
            })
            .map(|term| term.settled),
    )?;
    let outstanding = positive(exact::difference(source.settled, returned)?);
    Ok(
        serde_json::json!({"state":if source.settled.is_zero(){"awaiting_delivery"}else if outstanding.is_zero(){"returned"}else if at_ms>=loan.until_ms{"overdue"}else{"on_loan"},"settled":source.settled,"returned":returned,"outstanding":outstanding,"until_ms":loan.until_ms}),
    )
}
