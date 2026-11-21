use crate::{
    StoreError, config::transfer_reservation_default, transfer_stock::validate_all_on, write_tx,
};
use nucleus::{
    PromiseState,
    execution::now,
    new_uid,
    performance::{Span, add},
    transfer::{OpenPromiseReusePolicy, TransferLocationSnapshot},
};
use sqlx::{Row, SqlitePool, query, query_scalar, sqlite::SqliteRow};
use std::collections::HashMap;

/// A persisted promise and its reservation terms.
#[derive(Debug, Clone)]
pub struct PromiseRow {
    /// Promise UID.
    pub uid: String,
    /// Record carrying the promise.
    pub record_uid: Option<String>,
    /// Concept associated with the pledged amount.
    pub concept_uid: Option<String>,
    /// Unit of the amount.
    pub unit_uid: Option<String>,
    /// Pledged change in quantity.
    pub delta: f64,
    /// Earliest time the promise applies.
    pub window_start: Option<String>,
    /// End of the promise window.
    pub window_end: Option<String>,
    /// Location captured for the promise.
    pub location: Option<TransferLocationSnapshot>,
    /// Party associated with the promise.
    pub party_uid: Option<String>,
    /// Current lifecycle state.
    pub state: PromiseState,
    /// Condition on fulfillment.
    pub condition: Option<String>,
    /// Associated transfer Record UID.
    pub transfer_uid: Option<String>,
    /// Rule that created the promise.
    pub rule_uid: Option<String>,
    /// Reservation policy used for stock validation.
    pub reserve_from: String,
    /// Persisted revision number.
    pub revision: u64,
    /// Policy for reusing the open promise.
    pub open_reuse_policy: OpenPromiseReusePolicy,
}

/// Values accepted when creating a promise.
#[derive(Debug, Clone, Default)]
pub struct NewPromise {
    /// Optional Record carrying the promise.
    pub record_uid: Option<String>,
    /// Optional concept for the pledged amount.
    pub concept_uid: Option<String>,
    /// Pledged change in quantity.
    pub delta: f64,
    /// Optional end of the promise window.
    pub window_end: Option<String>,
    /// Optional party associated with the promise.
    pub party_uid: Option<String>,
    /// Initial state, proposed when omitted.
    pub state: Option<PromiseState>,
    /// Optional condition on fulfillment.
    pub condition: Option<String>,
    /// Optional associated transfer Record UID.
    pub transfer_uid: Option<String>,
    /// Optional originating rule.
    pub rule_uid: Option<String>,
    /// Explicit policy, otherwise inherited from the transfer or Store.
    pub reserve_from: Option<String>,
}

/// Creates a promise using its explicit or inherited reservation policy.
///
/// # Errors
///
/// Returns an error if storage or stock validation fails.
pub async fn insert_promise(pool: &SqlitePool, p: NewPromise) -> Result<String, StoreError> {
    let uid = new_uid("p");
    let now = now().to_rfc3339();
    let mut reserve_from = p.reserve_from;
    if reserve_from.is_none()
        && let Some(transfer_uid) = &p.transfer_uid
    {
        reserve_from = query_scalar::<_, Option<String>>(
            "SELECT reserve_default FROM transfer WHERE record_uid = ?",
        )
        .bind(transfer_uid)
        .fetch_optional(pool)
        .await?
        .flatten();
    }
    let reserve_from = match reserve_from {
        Some(value) => value,
        None => transfer_reservation_default(pool).await?,
    };
    let mut tx = write_tx(pool).await?;
    query(
        "INSERT INTO promise (uid, record_uid, concept_uid, delta, window_end, party_uid,
                              state, condition, transfer_uid, rule_uid, reserve_from,
                              created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(&p.record_uid)
    .bind(&p.concept_uid)
    .bind(p.delta)
    .bind(&p.window_end)
    .bind(&p.party_uid)
    .bind(p.state.unwrap_or(PromiseState::Proposed).as_str())
    .bind(&p.condition)
    .bind(&p.transfer_uid)
    .bind(&p.rule_uid)
    .bind(&reserve_from)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    validate_all_on(&mut tx).await?;
    tx.commit().await?;
    Ok(uid)
}

/// Reads eligible promises whose windows ended before the supplied time.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn expired_promises(
    pool: &SqlitePool,
    now_rfc3339: &str,
) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(query(
        "SELECT * FROM promise
          WHERE window_end IS NOT NULL AND window_end < ?
            AND state IN ('open', 'proposed', 'agreed', 'active')
          ORDER BY created_at",
    )
    .bind(now_rfc3339)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter_map(map_promise)
    .collect())
}

/// Updates a promise amount after validating stock reservations.
///
/// # Errors
///
/// Returns an error if storage or stock validation fails.
pub async fn set_promise_delta(pool: &SqlitePool, uid: &str, delta: f64) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    query("UPDATE promise SET delta = ?, updated_at = ? WHERE uid = ?")
        .bind(delta)
        .bind(now().to_rfc3339())
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    validate_all_on(&mut tx).await?;
    tx.commit().await
}

/// Reads valid promises for one Record.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn promises_for_record(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(query("SELECT * FROM promise WHERE record_uid = ?")
        .bind(record_uid)
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(map_promise)
        .collect())
}

/// Groups valid promises by the requested Record UIDs.
///
/// # Errors
///
/// Returns an error if UID encoding or the read fails.
pub async fn promises_for_records(
    pool: &SqlitePool,
    records: &[&str],
) -> Result<HashMap<String, Vec<PromiseRow>>, StoreError> {
    if records.is_empty() {
        return Ok(HashMap::new());
    }
    let selected =
        serde_json::to_string(records).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query("SELECT * FROM promise WHERE record_uid IN (SELECT value FROM json_each(?))")
        .bind(selected)
        .fetch_all(pool)
        .await?;
    let mut output: HashMap<String, Vec<PromiseRow>> = HashMap::new();
    for promise in rows.into_iter().filter_map(map_promise) {
        let Some(record) = &promise.record_uid else {
            continue;
        };
        if let Some(promises) = output.get_mut(record) {
            promises.push(promise);
        } else {
            output.insert(record.clone(), vec![promise]);
        }
    }
    Ok(output)
}

pub(crate) fn map_promise(row: SqliteRow) -> Option<PromiseRow> {
    let state = PromiseState::parse(row.try_get("state").ok()?)?;
    let open_reuse_policy = OpenPromiseReusePolicy::parse(row.try_get("open_reuse_policy").ok()?)?;
    let revision = u64::try_from(row.try_get::<i64, _>("revision").ok()?).ok()?;
    let lat: Option<f64> = row.try_get("location_lat").ok()?;
    let lon: Option<f64> = row.try_get("location_lon").ok()?;
    let address: Option<String> = row.try_get("location_address").ok()?;
    let location = (lat.is_some() || lon.is_some() || address.is_some())
        .then_some(TransferLocationSnapshot { lat, lon, address });
    Some(PromiseRow {
        uid: row.try_get("uid").ok()?,
        record_uid: row.try_get("record_uid").ok()?,
        concept_uid: row.try_get("concept_uid").ok()?,
        unit_uid: row.try_get("unit_uid").ok()?,
        delta: row.try_get("delta").ok()?,
        window_start: row.try_get("window_start").ok()?,
        window_end: row.try_get("window_end").ok()?,
        location,
        party_uid: row.try_get("party_uid").ok()?,
        state,
        condition: row.try_get("condition").ok()?,
        transfer_uid: row.try_get("transfer_uid").ok()?,
        rule_uid: row.try_get("rule_uid").ok()?,
        reserve_from: row.try_get("reserve_from").ok()?,
        revision,
        open_reuse_policy,
    })
}

/// Reads one promise if it exists and can be decoded.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn get_promise(pool: &SqlitePool, uid: &str) -> Result<Option<PromiseRow>, StoreError> {
    Ok(query("SELECT * FROM promise WHERE uid = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?
        .and_then(map_promise))
}

/// Lists valid promises in creation order.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn list_promises(pool: &SqlitePool) -> Result<Vec<PromiseRow>, StoreError> {
    Ok(query("SELECT * FROM promise ORDER BY created_at")
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(map_promise)
        .collect())
}

/// Stored terms used when forecasting a promise.
#[derive(Debug)]
pub struct PromiseProjection {
    /// Record carrying the promise, if any.
    pub record_uid: Option<String>,
    /// Pledged change in quantity.
    pub delta: f64,
    /// Optional end of the promise window.
    pub window_end: Option<String>,
    /// Current lifecycle state.
    pub state: PromiseState,
}

fn admitted_promise_state(row: &SqliteRow) -> Option<PromiseState> {
    OpenPromiseReusePolicy::parse(row.try_get("open_reuse_policy").ok()?)?;
    PromiseState::parse(row.try_get("state").ok()?)
}

/// Reads the promise fields needed for forecasting, in creation order.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn promise_projections(pool: &SqlitePool) -> Result<Vec<PromiseProjection>, StoreError> {
    let _read = Span::new("promise-projection-read-await");
    let rows = query(
        "SELECT record_uid, delta, window_end, state, open_reuse_policy
         FROM promise ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    add("promise-projection-rows", rows.len() as u64);
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(PromiseProjection {
                state: admitted_promise_state(&row)?,
                record_uid: row.try_get("record_uid").ok()?,
                delta: row.try_get("delta").ok()?,
                window_end: row.try_get("window_end").ok()?,
            })
        })
        .collect())
}

/// Reads a valid promise’s optional party. The outer option denotes a matching promise.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn promise_party(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<Option<String>>, StoreError> {
    Ok(
        query("SELECT party_uid, state, open_reuse_policy FROM promise WHERE uid = ?")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .and_then(|row| {
                admitted_promise_state(&row)?;
                row.try_get("party_uid").ok()
            }),
    )
}

/// Updates a promise state after validating stock reservations.
///
/// # Errors
///
/// Returns an error if storage or stock validation fails.
pub async fn set_promise_state(
    pool: &SqlitePool,
    uid: &str,
    state: PromiseState,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    query("UPDATE promise SET state = ?, updated_at = ? WHERE uid = ?")
        .bind(state.as_str())
        .bind(now().to_rfc3339())
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    validate_all_on(&mut tx).await?;
    tx.commit().await
}

/// Reads the state of a promise when the stored state is recognized.
///
/// # Errors
///
/// Returns an error if the read fails.
pub async fn promise_state(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<PromiseState>, StoreError> {
    Ok(
        query_scalar::<_, String>("SELECT state FROM promise WHERE uid = ?")
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .as_deref()
            .and_then(PromiseState::parse),
    )
}
