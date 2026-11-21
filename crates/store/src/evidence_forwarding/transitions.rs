use nucleus::{
    karma::CanonicalHash,
    trust_analysis::custody::{Acknowledgment, forwarding::MAX_ATTEMPTS},
};
use serde_json::to_string;
use sqlx::{Sqlite, Transaction, query};

use super::{get_on, invalid};
use crate::StoreError;

/// Returns interrupted deliveries to the queue when they have attempts left.
/// # Errors
/// Returns an error if the interrupted jobs cannot be updated.
pub async fn interrupted_on(tx: &mut Transaction<'_, Sqlite>, now: i64) -> Result<(), StoreError> {
    query("UPDATE record_evidence_forwarding SET state=CASE WHEN attempts>=3 THEN 'failed' ELSE 'pending' END,next_attempt=?,error='Interrupted delivery; no new retention acknowledgment is assumed' WHERE state='sending'")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Claims the ready delivery for the exact offer and advances its attempt count.
/// # Errors
/// Returns an error if the queue cannot be updated.
pub async fn claim_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    hash: &CanonicalHash,
    now: i64,
) -> Result<bool, StoreError> {
    let updated = query("UPDATE record_evidence_forwarding SET state='sending',attempts=attempts+1,sent_bytes=0,error=NULL WHERE record_uid=? AND offer_hash=? AND state='pending' AND attempts<3 AND next_attempt<=? AND json_extract(offer_json,'$.deliver_by')>?")
        .bind(record)
        .bind(hash.as_str())
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Records monotonic byte progress within the reviewed artifact size.
/// # Errors
/// Returns an error if the progress cannot be saved.
pub async fn progress_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    hash: &CanonicalHash,
    bytes: u32,
) -> Result<bool, StoreError> {
    let updated = query("UPDATE record_evidence_forwarding SET sent_bytes=? WHERE record_uid=? AND offer_hash=? AND state='sending' AND ?>=sent_bytes AND ?<=json_extract(offer_json,'$.bytes')")
        .bind(bytes)
        .bind(record)
        .bind(hash.as_str())
        .bind(bytes)
        .bind(bytes)
        .execute(&mut **tx)
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Saves the result for the exact delivery, including a valid receipt after cancellation.
/// # Errors
/// Returns an error if the job, result, or timestamp is invalid, or the update fails.
pub async fn result_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    hash: &CanonicalHash,
    result: Result<&Acknowledgment, &str>,
    now: i64,
) -> Result<bool, StoreError> {
    let job = get_on(tx, record).await?;
    if job.offer.hash().map_err(invalid)? != *hash
        || (job.state != "sending" && !(job.state == "cancelled" && result.is_ok()))
    {
        return Ok(false);
    }
    let (state, next, error, ack) = match result {
        Ok(ack) if ack.matches(&job.offer) => (
            "stored",
            now,
            (job.state == "cancelled").then_some(
                "Acknowledged after cancellation; the public copy was already delivered",
            ),
            Some(to_string(ack).map_err(invalid)?),
        ),
        Ok(_) => {
            return Err(invalid(
                "Custodian acknowledgment differs from the reviewed delivery",
            ));
        }
        Err(error) => (
            if job.attempts >= MAX_ATTEMPTS {
                "failed"
            } else {
                "pending"
            },
            now.checked_add(5 * i64::from(job.attempts))
                .ok_or_else(|| {
                    invalid("Evidence forwarding retry is outside the supported time range")
                })?,
            Some(bounded_error(error)),
            None,
        ),
    };
    let updated = query("UPDATE record_evidence_forwarding SET state=?,next_attempt=?,error=?,acknowledgment_json=? WHERE record_uid=? AND offer_hash=? AND state IN ('sending','cancelled')")
        .bind(state)
        .bind(next)
        .bind(error)
        .bind(ack)
        .bind(record)
        .bind(hash.as_str())
        .execute(&mut **tx)
        .await?;
    Ok(updated.rows_affected() == 1)
}

/// Holds a pending or sending delivery with a bounded diagnostic.
/// # Errors
/// Returns an error if the delivery cannot be updated.
pub async fn hold_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    error: &str,
) -> Result<(), StoreError> {
    query("UPDATE record_evidence_forwarding SET state='held',error=? WHERE record_uid=? AND state IN ('pending','sending')")
        .bind(bounded_error(error))
        .bind(record)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Cancels an eligible reviewed delivery for the exact owner and offer.
/// # Errors
/// Returns an error if the owner or offer does not match, or the job cannot be read or updated.
pub async fn cancel_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    owner: &str,
    hash: &CanonicalHash,
) -> Result<(), StoreError> {
    let job = get_on(tx, record).await?;
    if job.owner != owner || job.offer.hash().map_err(invalid)? != *hash {
        return Err(invalid("Review the current delivery and its exact owner"));
    }
    query("UPDATE record_evidence_forwarding SET state='cancelled',error='Cancelled by its operator' WHERE record_uid=? AND state IN ('pending','sending','held','failed')")
        .bind(record)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn bounded_error(error: &str) -> &str {
    error
        .char_indices()
        .nth(500)
        .and_then(|(end, _)| error.get(..end))
        .unwrap_or(error)
}
