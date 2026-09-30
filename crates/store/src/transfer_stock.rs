use chrono::{DateTime, Utc};
use nucleus::DecimalValue;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::{StoreError, exact};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Input {
    pub record: String,
    pub person: String,
    pub minimum: Option<DecimalValue>,
    pub expected_version: u64,
    pub request_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Limit {
    pub record: String,
    pub writer: String,
    pub minimum: DecimalValue,
    pub version: u64,
}

pub async fn get(pool: &SqlitePool, record: &str) -> Result<Option<Limit>, StoreError> {
    sqlx::query("SELECT * FROM transfer_stock_limit WHERE record_uid = ?")
        .bind(record)
        .fetch_optional(pool)
        .await?
        .map(|row| {
            Ok(Limit {
                record: row.get("record_uid"),
                writer: row.get("writer_cell_uid"),
                minimum: exact::read_decimal(&row, "minimum")?,
                version: row.get::<i64, _>("version") as u64,
            })
        })
        .transpose()
}

pub async fn version(pool: &SqlitePool, record: &str) -> Result<u64, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transfer_stock_limit_event WHERE record_uid = ?")
            .bind(record)
            .fetch_one(pool)
            .await?;
    Ok(count as u64)
}

async fn writer(connection: &mut SqliteConnection, cell: &str) -> Result<(), StoreError> {
    let current: Option<(String, String)> = sqlx::query_as("SELECT payload, not_after FROM organ_roster WHERE organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ')")
        .fetch_optional(&mut *connection).await?;
    if let Some((payload, expiry)) = current {
        let valid = DateTime::parse_from_rfc3339(&expiry)
            .is_ok_and(|at| at.with_timezone(&Utc) > nucleus::execution::now());
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        let writers: Vec<_> = payload["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|entry| {
                entry["capabilities"]
                    .as_array()
                    .is_some_and(|caps| caps.iter().any(|cap| cap == "write"))
            })
            .collect();
        if !valid || writers.len() != 1 || writers[0]["cell_uid"] != cell {
            return Err(invalid(
                "hard stock limits require a current Organ roster with this Cell as its only writer",
            ));
        }
    } else {
        let others: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE kind = 'device' AND deleted_at IS NULL AND organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ') AND uid != ?)")
            .bind(cell).fetch_one(&mut *connection).await?;
        if others {
            return Err(invalid("hard stock limits require one writing Cell"));
        }
    }
    let old: Vec<(String,String)> = sqlx::query_as("SELECT payload, not_after FROM transfer_stock_roster_history WHERE organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ')")
        .fetch_all(&mut *connection).await?;
    for (payload, expiry) in old {
        if DateTime::parse_from_rfc3339(&expiry)
            .is_ok_and(|at| at.with_timezone(&Utc) <= nucleus::execution::now())
        {
            continue;
        }
        let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
        if payload["cells"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|entry| {
                entry["cell_uid"] != cell
                    && entry["capabilities"]
                        .as_array()
                        .is_some_and(|caps| caps.iter().any(|cap| cap == "write"))
            })
        {
            return Err(invalid(
                "another Cell still has an unexpired write authorization; hard stock limits must wait for it to expire",
            ));
        }
    }
    Ok(())
}

pub async fn set<F>(
    pool: &SqlitePool,
    input: Input,
    now: DateTime<Utc>,
    key_id: &str,
    public_key: &str,
    sign: F,
) -> Result<String, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.request_id.trim().is_empty()
        || input.request_id.len() > 200
        || input.minimum.is_some_and(|value| value.is_negative())
        || input.expected_version >= i64::MAX as u64
    {
        return Err(invalid(
            "a stock limit needs a nonnegative minimum and a request identifier",
        ));
    }
    let payload = serde_json::to_string(&input).map_err(invalid)?;
    let mut tx = crate::write_tx(pool).await?;
    if let Some((uid, previous)) = sqlx::query_as::<_, (String, String)>(
        "SELECT uid, payload FROM transfer_stock_limit_event WHERE request_id = ?",
    )
    .bind(&input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        if previous != payload {
            return Err(invalid("stock limit request reused with different values"));
        }
        tx.rollback().await?;
        return Ok(uid);
    }
    let own: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record r JOIN record p ON p.uid = ? JOIN record o ON o.slug = 'local-organ' AND o.kind = 'organ' WHERE r.uid = ? AND r.organ_uid = o.uid AND p.organ_uid = o.uid AND p.kind = 'person' AND p.deleted_at IS NULL AND r.deleted_at IS NULL AND r.kind = 'plain')")
        .bind(&input.person).bind(&input.record).fetch_one(&mut *tx).await?;
    if !own {
        return Err(invalid(
            "stock limits belong to your own live resource Records",
        ));
    }
    let current: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM transfer_stock_limit_event WHERE record_uid = ?")
            .bind(&input.record)
            .fetch_one(&mut *tx)
            .await?;
    if current as u64 != input.expected_version {
        return Err(invalid("the stock limit changed; review it again"));
    }
    let cell: String =
        sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-cell' AND kind = 'device'")
            .fetch_one(&mut *tx)
            .await?;
    let prior: Option<String> =
        sqlx::query_scalar("SELECT writer_cell_uid FROM transfer_stock_limit WHERE record_uid = ?")
            .bind(&input.record)
            .fetch_optional(&mut *tx)
            .await?;
    if prior.is_some_and(|prior| prior != cell) {
        return Err(invalid("change the stock limit on its writing Cell"));
    }
    if input.minimum.is_some() {
        writer(&mut tx, &cell).await?;
    }
    let uid = nucleus::new_uid("tsl");
    let hash = nucleus::fact::sha256_hex(payload.as_bytes());
    let signature =
        sign(&hash).ok_or_else(|| invalid("stock limits require the owner's signature"))?;
    sqlx::query("INSERT INTO transfer_stock_limit_event (uid, record_uid, person_uid, payload, signature, key_id, public_key, request_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&uid).bind(&input.record).bind(&input.person).bind(payload).bind(signature).bind(key_id).bind(public_key).bind(input.request_id).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    if let Some(minimum) = input.minimum {
        let (mantissa, scale) = exact::decimal_columns(minimum);
        sqlx::query("INSERT INTO transfer_stock_limit (record_uid, writer_cell_uid, person_uid, minimum_mantissa, minimum_scale, version, event_uid) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(record_uid) DO UPDATE SET minimum_mantissa = excluded.minimum_mantissa, minimum_scale = excluded.minimum_scale, version = excluded.version, person_uid = excluded.person_uid, event_uid = excluded.event_uid")
            .bind(&input.record).bind(cell).bind(&input.person).bind(mantissa).bind(scale).bind(current+1).bind(&uid).execute(&mut *tx).await?;
        validate_record_on(&mut tx, &input.record, None).await?;
    } else {
        sqlx::query("DELETE FROM transfer_stock_limit WHERE record_uid = ?")
            .bind(&input.record)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(uid)
}

pub async fn require_current_writer_on(
    connection: &mut SqliteConnection,
) -> Result<(), StoreError> {
    let row: Option<(String,String,String)> = sqlx::query_as("SELECT roster.payload, roster.not_after, cell.uid FROM organ_roster roster JOIN record organ ON organ.uid = roster.organ_uid AND organ.slug = 'local-organ' JOIN record cell ON cell.slug = 'local-cell' AND cell.kind = 'device'")
        .fetch_optional(&mut *connection).await?;
    let Some((payload, expiry, cell)) = row else {
        return Ok(());
    };
    let payload: Value = serde_json::from_str(&payload).map_err(invalid)?;
    let allowed = payload["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|entry| {
            entry["cell_uid"] == cell
                && entry["capabilities"]
                    .as_array()
                    .is_some_and(|caps| caps.iter().any(|cap| cap == "write"))
        });
    if !allowed
        || !DateTime::parse_from_rfc3339(&expiry)
            .is_ok_and(|at| at.with_timezone(&Utc) > nucleus::execution::now())
    {
        return Err(invalid(
            "this Cell needs a current write authorization before changing resources or commitments",
        ));
    }
    Ok(())
}

pub async fn validate_record_on(
    connection: &mut SqliteConnection,
    record: &str,
    imported_writer: Option<&str>,
) -> Result<(), StoreError> {
    if imported_writer.is_none() {
        require_current_writer_on(connection).await?;
    }
    let Some(row) = sqlx::query("SELECT * FROM transfer_stock_limit WHERE record_uid = ?")
        .bind(record)
        .fetch_optional(&mut *connection)
        .await?
    else {
        return Ok(());
    };
    let authority: String = row.get("writer_cell_uid");
    if let Some(imported_writer) = imported_writer {
        if imported_writer != authority {
            return Err(invalid(
                "this stock change was authored by a Cell without stock authority",
            ));
        }
        return Ok(());
    }
    let local: Option<String> =
        sqlx::query_scalar("SELECT uid FROM record WHERE slug = 'local-cell' AND kind = 'device'")
            .fetch_optional(&mut *connection)
            .await?;
    if local.as_deref() != Some(&authority) {
        return Err(invalid("change hard-reserved stock on its writing Cell"));
    }
    writer(connection, &authority).await?;
    let balance = crate::transfer_balances::read_on(connection, record).await?;
    if !balance.incomplete.is_empty() {
        return Err(invalid(
            "hard stock limit cannot verify every remaining commitment",
        ));
    }
    let minimum = exact::read_decimal(&row, "minimum")?;
    if balance.surplus.exact_numeric_cmp(minimum).is_lt() {
        return Err(invalid(format!(
            "hard stock limit: remaining surplus {} would be below {minimum}",
            balance.surplus
        )));
    }
    Ok(())
}

pub async fn validate_all_on(connection: &mut SqliteConnection) -> Result<(), StoreError> {
    require_current_writer_on(connection).await?;
    let records: Vec<String> = sqlx::query_scalar("SELECT record_uid FROM transfer_stock_limit")
        .fetch_all(&mut *connection)
        .await?;
    for record in records {
        validate_record_on(connection, &record, None).await?;
    }
    Ok(())
}
