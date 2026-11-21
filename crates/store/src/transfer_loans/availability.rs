//! Project temporary borrowing and returns onto each Record's availability.

use std::collections::{BTreeMap, BTreeSet};

use nucleus::{DecimalValue, execution::current};
use serde::Serialize;
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::{
    effects::{Effect, effects},
    invalid, minimum, positive,
    terms::{Terms, reference_key, terms_on},
};
use crate::{StoreError, exact, transfer_accounting::calculate};

type Route<'a> = (&'a str, &'a str, &'a str);
struct Receipt {
    settled: DecimalValue,
    people: BTreeSet<String>,
}
type Receipts = BTreeMap<String, BTreeMap<String, Receipt>>;

/// Temporary availability adjustment projected from loans and returns.
#[derive(Clone, Debug, Serialize)]
pub struct Adjustment {
    /// Record whose availability is adjusted.
    pub record: String,
    /// Signed adjustment without changing the physical ledger.
    pub delta: DecimalValue,
    /// Next loan window boundary after projection time.
    pub next_ms: Option<i64>,
    /// Whether the ledger evidence disagrees with the current unit.
    pub unit_changed: bool,
}

/// Project loans and returns at a specific instant.
///
/// # Errors
///
/// Returns an error if terms or effect evidence cannot be read, or quantity math fails.
pub async fn adjustments(pool: &SqlitePool, at_ms: i64) -> Result<Vec<Adjustment>, StoreError> {
    adjustments_on(&mut *pool.acquire().await?, at_ms).await
}

/// Project loans and returns on the caller's connection.
///
/// # Errors
///
/// Returns an error if terms or effect evidence cannot be read, or quantity math fails.
pub async fn adjustments_on(
    connection: &mut SqliteConnection,
    at_ms: i64,
) -> Result<Vec<Adjustment>, StoreError> {
    let simulated = current().is_some();
    let mut terms = terms_on(connection, true).await?;
    if !terms
        .iter()
        .any(|terms| terms.accepted.is_some() || simulated && terms.item.loan.is_some())
    {
        return Ok(Vec::new());
    }
    let (has_timing, has_receipts) = if simulated {
        sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'simulation_loan_timing'),
                    EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'simulation_assumed_transfer_effect')",
        )
        .fetch_one(&mut *connection)
        .await?
    } else {
        (false, false)
    };
    let effects = effects(connection, has_receipts).await?;
    let assumed_receivers = if has_receipts {
        apply_receipts(connection, &mut terms).await?
    } else {
        Receipts::new()
    };

    let mut by_route = BTreeMap::<Route<'_>, Vec<&Effect>>::new();
    let mut by_record = BTreeMap::<(&str, &str), Vec<&Effect>>::new();
    for effect in &effects {
        by_route
            .entry((&effect.transfer, &effect.exchange, &effect.person))
            .or_default()
            .push(effect);
        by_record
            .entry((&effect.person, &effect.record))
            .or_default()
            .push(effect);
    }
    let mut returns = BTreeMap::<Route<'_>, Vec<&Terms>>::new();
    for term in &terms {
        if let Some(reference) = &term.item.return_of {
            returns
                .entry(reference_key(reference))
                .or_default()
                .push(term);
        }
    }

    let mut result = BTreeMap::<&str, Adjustment>::new();
    let mut fulfilments = BTreeMap::new();
    for source in &terms {
        let Some(route) = &source.item.exchange else {
            continue;
        };
        let receiver = route.receiver.as_str();
        let transfer = source.transfer.as_str();
        let exchange = route.uid.as_str();
        let key = (transfer, exchange, receiver);
        let (from_ms, mut until_ms) = if let Some(loan) = &source.accepted {
            (loan.from_ms, loan.until_ms)
        } else if let Some(interval) = &source.item.loan
            && assumed_receivers
                .get(transfer)
                .and_then(|exchanges| exchanges.get(exchange))
                .is_some_and(|receipt| receipt.people.contains(receiver))
        {
            interval.bounds().map_err(invalid)?
        } else {
            continue;
        };
        let timing = if has_timing {
            sqlx::query(
                "SELECT until_ms, returned_mantissa, returned_scale FROM simulation_loan_timing
                 WHERE origin = ? AND transfer_uid = ? AND exchange_uid = ? AND person_uid = ?",
            )
            .bind(&source.origin)
            .bind(transfer)
            .bind(exchange)
            .bind(receiver)
            .fetch_optional(&mut *connection)
            .await?
        } else {
            None
        };
        let mut assumed_return = exact::zero();
        if let Some(row) = timing {
            until_ms = row.get("until_ms");
            assumed_return = exact::read_decimal(&row, "returned")?;
        }

        let linked = returns
            .get(&(source.origin.as_str(), transfer, exchange))
            .map_or(&[][..], Vec::as_slice);
        let returned = max(
            assumed_return,
            exact::sum_exact(linked.iter().map(|term| term.settled))?,
        );
        let return_routes: BTreeSet<_> = linked
            .iter()
            .filter_map(|term| {
                term.item
                    .exchange
                    .as_ref()
                    .map(|route| (term.transfer.as_str(), route.uid.as_str()))
            })
            .collect();
        let mut contributions = BTreeMap::new();
        for effect in by_route.get(&key).into_iter().flatten() {
            let (delta, fulfilment) = contributions
                .entry(effect.record.as_str())
                .or_insert((exact::zero(), false));
            *delta = exact::sum_exact([*delta, effect.delta])?;
            *fulfilment |= effect.fulfilment;
        }
        let next_ms = [from_ms, until_ms]
            .into_iter()
            .filter(|time| *time > at_ms)
            .min();
        let return_formula = (!contributions.is_empty() && !source.settled.is_zero()).then(|| {
            format!(
                "incoming() * {} / {}",
                minimum(returned, source.settled),
                source.settled
            )
        });
        for (record, (credit, fulfilment)) in contributions {
            let credit = positive(credit);
            let returned_credit = if let Some(formula) = &return_formula {
                calculate(formula, credit, exact::zero())?.0
            } else {
                exact::zero()
            };
            let removed = if (from_ms..until_ms).contains(&at_ms) {
                returned_credit
            } else {
                credit
            };
            let effects = by_record.get(&(receiver, record)).into_iter().flatten();
            let is_return = |effect: &Effect| {
                return_routes.contains(&(effect.transfer.as_str(), effect.exchange.as_str()))
            };
            let physical_debit = positive(exact::negate(exact::sum_exact(
                effects
                    .clone()
                    .filter(|effect| is_return(effect))
                    .map(|effect| effect.delta),
            )?)?);
            let units: BTreeSet<_> = effects
                .filter(|effect| {
                    !effect.delta.is_zero()
                        && (effect.transfer == transfer && effect.exchange == exchange
                            || is_return(effect))
                })
                .map(|effect| effect.unit.as_deref())
                .collect();
            let unit: Option<String> =
                sqlx::query_scalar("SELECT unit_uid FROM record WHERE uid = ?")
                    .bind(record)
                    .fetch_one(&mut *connection)
                    .await?;
            let unit_changed = !credit.is_zero()
                && (units.len() > 1
                    || credit.exact_numeric_cmp(physical_debit).is_gt()
                        && units.first() != Some(&unit.as_deref()));
            let delta = exact::negate(positive(exact::difference(removed, physical_debit)?))?;
            if fulfilment {
                let active = positive(exact::difference(credit, max(removed, physical_debit))?);
                let total = fulfilments.entry(record).or_insert_with(exact::zero);
                *total = exact::sum_exact([*total, active])?;
            }
            let adjustment = result.entry(record).or_insert_with(|| Adjustment {
                record: record.into(),
                delta: exact::zero(),
                next_ms: None,
                unit_changed: false,
            });
            adjustment.unit_changed |= unit_changed;
            adjustment.delta = exact::sum_exact([adjustment.delta, delta])?;
            adjustment.next_ms = adjustment.next_ms.into_iter().chain(next_ms).min();
        }
    }
    for (record, credit) in fulfilments {
        let Some(adjustment) = result.get_mut(record) else {
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

fn max(a: DecimalValue, b: DecimalValue) -> DecimalValue {
    if a.exact_numeric_cmp(b).is_gt() { a } else { b }
}

async fn apply_receipts(
    connection: &mut SqliteConnection,
    terms: &mut [Terms],
) -> Result<Receipts, StoreError> {
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
    let mut receipts = Receipts::new();
    for ((transfer, exchange, person), amount) in people {
        let receipt = receipts
            .entry(transfer)
            .or_default()
            .entry(exchange)
            .or_insert_with(|| Receipt {
                settled: exact::zero(),
                people: BTreeSet::new(),
            });
        receipt.people.insert(person);
        if amount.exact_numeric_cmp(receipt.settled).is_gt() {
            receipt.settled = amount;
        }
    }
    for source in terms {
        if let Some(route) = &source.item.exchange
            && let Some(receipt) = receipts
                .get(&source.transfer)
                .and_then(|exchanges| exchanges.get(&route.uid))
            && receipt.settled.exact_numeric_cmp(source.settled).is_gt()
        {
            source.settled = receipt.settled;
        }
    }
    Ok(receipts)
}
