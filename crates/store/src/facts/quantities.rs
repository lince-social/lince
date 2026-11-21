use chrono::{DateTime, TimeDelta, Utc};
use nucleus::DecimalValue;
use serde_json::{Value, from_str};
use sqlx::{Row, SqlitePool, query, query_scalar};

use super::{instant, rows::read_delta};
use crate::{
    StoreError,
    exact::{difference, zero},
};

#[derive(Clone, Copy)]
enum SignFilter {
    All,
    Positive,
    Negative,
}

impl SignFilter {
    fn keeps(self, value: DecimalValue) -> bool {
        match self {
            Self::All => true,
            Self::Positive => value.is_positive(),
            Self::Negative => value.is_negative(),
        }
    }
}

async fn fold_window(
    pool: &SqlitePool,
    record_uid: &str,
    start: &str,
    end: Option<&str>,
    filter: SignFilter,
) -> Result<DecimalValue, StoreError> {
    let rows = match end {
        Some(end) => {
            query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND at >= ? AND at < ?",
            )
            .bind(record_uid)
            .bind(start)
            .bind(end)
            .fetch_all(pool)
            .await?
        }
        None => {
            query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND at >= ?",
            )
            .bind(record_uid)
            .bind(start)
            .fetch_all(pool)
            .await?
        }
    };
    let mut total = zero();
    for row in rows {
        let delta = read_delta(&row)?;
        if filter.keeps(delta) {
            total = total
                .aligned_add(delta)
                .ok_or_else(|| StoreError::Decode("quantity sum overflows i128".into()))?;
        }
    }
    Ok(total)
}

fn seconds_before(at: DateTime<Utc>, seconds: i64) -> Result<DateTime<Utc>, StoreError> {
    TimeDelta::try_seconds(seconds)
        .and_then(|delta| at.checked_sub_signed(delta))
        .ok_or_else(|| {
            StoreError::Protocol("Fact window falls outside the supported calendar".into())
        })
}

/// Sums exact changes at or after the window's lower bound.
///
/// # Errors
/// Returns an error for an invalid calendar bound, failed read, or overflow.
pub async fn sum_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let cutoff = instant(seconds_before(now, window_secs)?);
    fold_window(pool, record_uid, &cutoff, None, SignFilter::All).await
}

/// Sums positive exact changes at or after the window's lower bound.
///
/// # Errors
/// Returns an error for an invalid calendar bound, failed read, or overflow.
pub async fn sum_pos_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let cutoff = instant(seconds_before(now, window_secs)?);
    fold_window(pool, record_uid, &cutoff, None, SignFilter::Positive).await
}

/// Sums negative exact changes at or after the window's lower bound.
///
/// # Errors
/// Returns an error for an invalid calendar bound, failed read, or overflow.
pub async fn sum_neg_window(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let cutoff = instant(seconds_before(now, window_secs)?);
    fold_window(pool, record_uid, &cutoff, None, SignFilter::Negative).await
}

/// Sums changes in a lagged time window with an exclusive upper bound.
///
/// # Errors
/// Returns an error for an invalid calendar bound, failed read, or overflow.
pub async fn sum_window_lagged(
    pool: &SqlitePool,
    record_uid: &str,
    window_secs: i64,
    end_lag_secs: i64,
    now: DateTime<Utc>,
) -> Result<DecimalValue, StoreError> {
    let end = seconds_before(now, end_lag_secs)?;
    let start = instant(seconds_before(end, window_secs)?);
    fold_window(
        pool,
        record_uid,
        &start,
        Some(&instant(end)),
        SignFilter::All,
    )
    .await
}

/// Computes the current level from the latest usable checkpoint, later changes, and the offset.
///
/// # Errors
/// Returns an error if stored quantities cannot be read or the exact level overflows.
pub async fn level(pool: &SqlitePool, record_uid: &str) -> Result<DecimalValue, StoreError> {
    let (base, after_rowid) = level_anchor(pool, record_uid).await?;
    let rows = match after_rowid {
        Some(rowid) => {
            query(
                "SELECT delta_mantissa, delta_scale FROM fact
                  WHERE record_uid = ? AND rowid > ?",
            )
            .bind(record_uid)
            .bind(rowid)
            .fetch_all(pool)
            .await?
        }
        None => {
            query("SELECT delta_mantissa, delta_scale FROM fact WHERE record_uid = ?")
                .bind(record_uid)
                .fetch_all(pool)
                .await?
        }
    };
    let mut total = base;
    for row in rows {
        let delta = read_delta(&row)?;
        total = total.aligned_add(delta).ok_or_else(|| {
            StoreError::Decode(format!("level of {record_uid} overflows i128").into())
        })?;
    }
    total
        .aligned_add(quantity_offset(pool, record_uid, None).await?)
        .ok_or_else(|| StoreError::Decode("quantity level overflows i128".into()))
}

/// Reads the current or time-bounded quantity offset, or zero when there is no offset.
///
/// # Errors
/// Returns an error if the stored offset cannot be read as an exact decimal.
pub async fn quantity_offset(
    pool: &SqlitePool,
    record_uid: &str,
    at: Option<DateTime<Utc>>,
) -> Result<DecimalValue, StoreError> {
    let raw: Option<String> = if let Some(at) = at {
        query_scalar("SELECT json_extract(value, '$.value.offset') FROM sync_op WHERE tbl = 'record' AND uid = ? AND field = 'property:quantity' AND (json_extract(value, '$.clock') >> 16) <= ? ORDER BY json_extract(value, '$.clock') DESC, json_extract(value, '$.peer') DESC, json_extract(value, '$.change_uid') DESC LIMIT 1")
            .bind(record_uid).bind(at.timestamp_millis()).fetch_optional(pool).await?
    } else {
        query_scalar("SELECT json_extract(value, '$.offset') FROM record_property WHERE record_uid = ? AND property = 'quantity'").bind(record_uid).fetch_optional(pool).await?
    };
    match raw {
        Some(raw) => DecimalValue::parse_inferred(&raw)
            .map_err(|error| StoreError::Decode(error.to_string().into())),
        None => Ok(zero()),
    }
}

pub(crate) fn checkpoint_base(
    value: &Value,
    level: DecimalValue,
) -> Result<DecimalValue, StoreError> {
    match value.get("quantity_offset").and_then(Value::as_str) {
        Some(offset) => difference(
            level,
            DecimalValue::parse_inferred(offset)
                .map_err(|error| StoreError::Decode(error.to_string().into()))?,
        ),
        None => Ok(level),
    }
}

async fn level_anchor(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<(DecimalValue, Option<i64>), StoreError> {
    let rows = query(
        "SELECT rowid, payload FROM fact
          WHERE record_uid = ? AND cause_kind = 'checkpoint'
          ORDER BY rowid DESC LIMIT 32",
    )
    .bind(record_uid)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let Some(payload) = row.try_get::<Option<&str>, _>("payload")? else {
            continue;
        };
        let Ok(json) = from_str::<Value>(payload) else {
            continue;
        };
        let Some(text) = json.get("level").and_then(Value::as_str) else {
            continue;
        };
        let level = DecimalValue::parse_inferred(text).map_err(|error| {
            StoreError::Decode(format!("checkpoint level is not an exact decimal: {error}").into())
        })?;
        return Ok((checkpoint_base(&json, level)?, Some(row.try_get("rowid")?)));
    }
    Ok((zero(), None))
}
