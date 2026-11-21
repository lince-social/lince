//! Projects actual stock, private Transfer commitments, and stock available to offer.

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
};

use nucleus::DecimalValue;
use serde::Serialize;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::{StoreError, exact};

mod commitment;
mod inputs;
mod occurrence;
mod promise;

use inputs::{Inputs, record_ids};

fn invalid(error: impl Display) -> StoreError {
    StoreError::Protocol(error.to_string())
}

/// One Person's outstanding private application of a Promise to a Record.
#[derive(Clone, Debug, Serialize)]
pub struct Commitment {
    /// Transfer that supplied the terms.
    pub transfer: String,
    /// Display title from the Promise or its Transfer.
    pub title: String,
    /// Revision from which these terms were read.
    pub revision: u64,
    /// Promise selected for this exchange and Person.
    pub promise: String,
    /// Exchange within the Transfer.
    pub exchange: String,
    /// Activated occurrence, or no occurrence for unactivated terms.
    pub occurrence: Option<String>,
    /// Person responsible for the private application.
    pub person: String,
    /// Record whose balance includes this commitment.
    pub record: String,
    /// Record receiving the primary private effect.
    pub application_record: String,
    /// Canonical unit used by the Transfer.
    pub unit: Option<String>,
    /// Canonical quantity still to apply.
    pub canonical_remaining: DecimalValue,
    /// Signed quantity change still to apply to this Record.
    pub private_remaining: DecimalValue,
    /// Whether the Person gives the canonical quantity.
    pub outgoing: bool,
    /// Quantity held back by the terms' reservation rule.
    pub reserved: DecimalValue,
    /// State of the selected Promise.
    pub state: String,
    /// End of the applicable delivery window.
    pub due: Option<String>,
    /// Group whose unstarted alternatives share reservations.
    pub alternative_group: Option<String>,
}

/// A Record's exact stock and outstanding Transfer commitments.
#[derive(Clone, Debug, Serialize)]
pub struct Balance {
    /// Quantity recorded in the ledger.
    pub actual: DecimalValue,
    /// Ledger quantity adjusted for current loans.
    pub available: DecimalValue,
    /// Quantity held back by commitments.
    pub reserved: DecimalValue,
    /// Available quantity after reservations, possibly negative.
    pub surplus: DecimalValue,
    /// Nonnegative stock available to offer under the current unit.
    pub can_offer: DecimalValue,
    /// Outstanding commitments in Transfer, exchange, Person, and path order.
    pub commitments: Vec<Commitment>,
    /// Diagnostics for stock or commitments that could not be fully projected.
    pub incomplete: Vec<String>,
}

/// Sum reservations, using the largest unstarted alternative per group and Person.
///
/// When a group has started, sum its active and kept alternatives instead.
///
/// # Errors
///
/// Returns an error if a reservation total exceeds the supported decimal range.
pub fn reserved_for<'a>(
    commitments: impl IntoIterator<Item = &'a Commitment>,
) -> Result<DecimalValue, StoreError> {
    let mut reservations = Vec::new();
    let mut alternatives: BTreeMap<_, BTreeMap<_, _>> = BTreeMap::new();
    for commitment in commitments {
        if let Some(group) = &commitment.alternative_group {
            let (amount, active) = alternatives
                .entry((group.as_str(), commitment.person.as_str()))
                .or_default()
                .entry(commitment.transfer.as_str())
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
    exact::sum_exact(reservations)
}

/// Read one Record's balance in a consistent transaction.
///
/// # Errors
///
/// Returns an error if the Record is unavailable or a read or stock calculation fails.
pub async fn read(pool: &SqlitePool, record: &str) -> Result<Balance, StoreError> {
    let mut tx = pool.begin().await?;
    let balance = read_on(&mut tx, record).await?;
    tx.rollback().await?;
    Ok(balance)
}

/// Read one Record's balance on the caller's connection.
///
/// # Errors
///
/// Returns an error if the Record is unavailable or a read or stock calculation fails.
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
    let inputs = Inputs::load(connection, &record_ids(&[record])?).await?;
    inputs
        .scope(connection, &[record])
        .await?
        .balance_on(connection, record, actual)
        .await
}

/// Read several Record balances from the same snapshot and shared inputs.
///
/// # Errors
///
/// Returns an error if any Record is unavailable or a read or stock calculation fails.
pub async fn read_many(
    pool: &SqlitePool,
    records: &[&str],
) -> Result<HashMap<String, Balance>, StoreError> {
    if records.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = record_ids(records)?;
    let mut tx = pool.begin().await?;
    let rows = sqlx::query(
        "SELECT uid, quantity_mantissa, quantity_scale FROM record
         WHERE deleted_at IS NULL AND uid IN (SELECT value FROM json_each(?))",
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await?;
    let mut actuals = HashMap::<String, _>::with_capacity(rows.len());
    for row in rows {
        actuals.insert(row.get("uid"), exact::read_decimal(&row, "quantity")?);
    }
    if records.iter().any(|record| !actuals.contains_key(*record)) {
        return Err(StoreError::RowNotFound);
    }
    let inputs = Inputs::load(&mut tx, &ids).await?;
    let mut scope = inputs.scope(&mut tx, records).await?;
    let mut balances = HashMap::with_capacity(actuals.len());
    for (record, actual) in actuals {
        let balance = scope.balance_on(&mut tx, &record, actual).await?;
        balances.insert(record, balance);
    }
    tx.rollback().await?;
    Ok(balances)
}
