//! Read local and delivered loan terms and validate linked promises.

use std::collections::{BTreeMap, BTreeSet};

use nucleus::{
    DecimalValue,
    transfer::{disclosure::TransferItem, exchange::ExchangeRoute, loans::Reference},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{Accepted, invalid, positive};
use crate::{StoreError, exact, transfer_accounting::amount};

/// A local or delivered loan exchange and its settlement state.
#[derive(Clone, Debug, Serialize)]
pub struct Terms {
    /// Organ that originated the Transfer.
    pub origin: String,
    /// Transfer holding the exchange.
    pub transfer: String,
    /// Reviewed item, route, and linked terms.
    pub item: TransferItem,
    /// Latest accepted interval for the exchange.
    pub accepted: Option<Accepted>,
    /// Canonical quantity settled for the exchange.
    pub settled: DecimalValue,
    /// Whether delivery access has ended for these retained terms.
    pub retained: bool,
}

/// Read local and delivered loan terms, including revoked deliveries when requested.
///
/// # Errors
///
/// Returns an error if saved terms, settlements, or intervals cannot be read.
pub async fn terms(pool: &SqlitePool, retained: bool) -> Result<Vec<Terms>, StoreError> {
    terms_on(&mut *pool.acquire().await?, retained).await
}

pub(super) async fn terms_on(
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
            .bind(row.get::<&str,_>("uid")).fetch_one(&mut *connection).await?;
        let settled = amount(settled)?;
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
        let promises = view["promises"].as_array().map_or(&[][..], Vec::as_slice);
        let mut members = BTreeMap::<&str, BTreeSet<&str>>::new();
        for promise in promises {
            if let (Some(exchange), Some(uid)) =
                (promise["exchange"].as_str(), promise["uid"].as_str())
            {
                members.entry(exchange).or_default().insert(uid);
            }
        }
        for promise in promises {
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
                exchange: Some(ExchangeRoute {
                    uid: exchange.into(),
                    giver: giver.into(),
                    receiver: receiver.into(),
                }),
                loan: Deserialize::deserialize(&promise["loan"]).map_err(invalid)?,
                return_of: Deserialize::deserialize(&promise["return_of"]).map_err(invalid)?,
                ..Default::default()
            };
            if item.loan.is_none() && item.return_of.is_none() {
                continue;
            }
            let accepted = Deserialize::deserialize(&promise["accepted_loan"]).map_err(invalid)?;
            let mut per_promise = BTreeMap::<&str, DecimalValue>::new();
            for occurrence in view["occurrences"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|occurrence| occurrence["disputed"] != true)
            {
                let Some(uid) = occurrence["promise"].as_str() else {
                    continue;
                };
                if !members.get(exchange).is_some_and(|uids| uids.contains(uid)) {
                    continue;
                }
                let quantity = amount(
                    occurrence["settlement_progress"]["settled_quantity"]
                        .as_f64()
                        .unwrap_or(0.0),
                )?;
                let total = per_promise.entry(uid).or_insert_with(exact::zero);
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
                    retained: row.get::<&str, _>("state") != "active",
                },
            );
        }
    }
    Ok(result.into_values().collect())
}

/// Check a linked return or future Need against the available accepted loan.
///
/// # Errors
///
/// Returns an error if the acting Person lacks access or the linked terms are invalid.
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

pub(super) fn reference_key(reference: &Reference) -> (&str, &str, &str) {
    (&reference.origin, &reference.transfer, &reference.exchange)
}

/// Summarize the accepted loan's settled, returned, and outstanding quantities.
///
/// # Errors
///
/// Returns an error if an exact quantity calculation overflows.
pub fn status(source: &Terms, all: &[Terms], at_ms: i64) -> Result<Value, StoreError> {
    let (Some(loan), Some(route)) = (&source.accepted, &source.item.exchange) else {
        return Ok(Value::Null);
    };
    let key = (
        source.origin.as_str(),
        source.transfer.as_str(),
        route.uid.as_str(),
    );
    let returned = exact::sum_exact(
        all.iter()
            .filter(|term| term.item.return_of.as_ref().map(reference_key) == Some(key))
            .map(|term| term.settled),
    )?;
    let outstanding = positive(exact::difference(source.settled, returned)?);
    let state = if source.settled.is_zero() {
        "awaiting_delivery"
    } else if outstanding.is_zero() {
        "returned"
    } else if at_ms >= loan.until_ms {
        "overdue"
    } else {
        "on_loan"
    };
    Ok(serde_json::json!({
        "state": state, "settled": source.settled, "returned": returned,
        "outstanding": outstanding, "until_ms": loan.until_ms,
    }))
}
