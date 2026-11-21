use super::protocol;
use crate::{StoreError, evidence_custody::cleanup_on, write_tx};
use serde_json::{Value, json};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, query, query_as, query_scalar};
use std::borrow::Cow;

const JOB_OVERHEAD: i64 = 512;
const QUEUE_LIMIT: i64 = 10_000;
const QUEUE_BYTES: i64 = 64 * 1024 * 1024;
const EXPIRY_GRACE_SECONDS: i64 = 600;

/// Queues a document and cancels older pending work for the same destination and identity.
///
/// # Errors
///
/// Returns an error for malformed identities, a full queue, or failed storage access.
pub async fn enqueue_on(
    tx: &mut Transaction<'_, Sqlite>,
    kind: &str,
    hash: &str,
    body: &str,
    destinations: &[String],
    expiry: i64,
) -> Result<(), StoreError> {
    let value: Value = serde_json::from_str(body).map_err(protocol)?;
    let id = match kind {
        "reply-admission" => {
            let mailbox = value
                .pointer("/document/mailbox")
                .and_then(Value::as_str)
                .ok_or_else(|| protocol("Invalid reply mailbox"))?;
            let sender = value
                .pointer("/document/sender_owner")
                .and_then(Value::as_str)
                .ok_or_else(|| protocol("Invalid reply sender"))?;
            Cow::Owned(format!("{mailbox}:{sender}"))
        }
        kind => {
            let path = match kind {
                "reply-route" => "/document/route/mailbox",
                "reply-control" => "/document/owner_key",
                "reply-ending" => "/document/id",
                "snippet" => "/id",
                "image" => "/hash",
                "posting-authority" => "/authority/owner_key",
                _ => "/authority/organ",
            };
            Cow::Borrowed(
                value
                    .pointer(path)
                    .and_then(Value::as_str)
                    .ok_or_else(|| protocol("Invalid publication identity"))?,
            )
        }
    };
    let body_bytes = i64::try_from(body.len()).unwrap_or(i64::MAX);
    for destination in destinations {
        let existing: bool = query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_publication_job WHERE hash=? AND destination=?)",
        )
        .bind(hash)
        .bind(destination)
        .fetch_one(&mut **tx)
        .await?;
        if existing {
            continue;
        }
        query("UPDATE social_publication_job SET state='cancelled',body='' WHERE kind=? AND destination=? AND hash<>? AND state='pending' AND CASE WHEN kind='snippet' THEN json_extract(body,'$.id') WHEN kind='image' THEN json_extract(body,'$.hash') WHEN kind='posting-authority' THEN json_extract(body,'$.authority.owner_key') WHEN kind='reply-route' THEN json_extract(body,'$.document.route.mailbox') WHEN kind='reply-control' THEN json_extract(body,'$.document.owner_key') WHEN kind='reply-ending' THEN json_extract(body,'$.document.id') WHEN kind='reply-admission' THEN json_extract(body,'$.document.mailbox') || ':' || json_extract(body,'$.document.sender_owner') ELSE json_extract(body,'$.authority.organ') END=?")
            .bind(kind).bind(destination).bind(hash).bind(id.as_ref()).execute(&mut **tx).await?;
        let (count, bytes): (i64, i64) = query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))+512),0) FROM social_publication_job")
            .fetch_one(&mut **tx).await?;
        if count >= QUEUE_LIMIT
            || bytes
                .saturating_add(body_bytes)
                .saturating_add(JOB_OVERHEAD)
                > QUEUE_BYTES
        {
            return Err(protocol(
                "The publication queue is full; remove expired work before publishing",
            ));
        }
        query("INSERT INTO social_publication_job(hash,destination,body,kind,expires_at) VALUES (?,?,?,?,?) ON CONFLICT DO NOTHING")
            .bind(hash).bind(destination).bind(body).bind(kind).bind(expiry).execute(&mut **tx).await?;
    }
    Ok(())
}

/// Lists up to 100 recent jobs with receipt metadata for the publication view.
///
/// # Errors
///
/// Returns storage or result-decoding errors.
pub async fn jobs(pool: &SqlitePool) -> Result<Vec<Value>, StoreError> {
    query("SELECT hash,destination,kind,state,error,attempts,receipt FROM social_publication_job ORDER BY rowid DESC LIMIT 100")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            let receipt = row.try_get::<Option<&str>, _>("receipt")?
                .and_then(|body| serde_json::from_str::<Value>(body).ok());
            Ok(json!({
                "hash": row.try_get::<&str, _>("hash")?,
                "destination": row.try_get::<&str, _>("destination")?,
                "kind": row.try_get::<&str, _>("kind")?,
                "state": row.try_get::<&str, _>("state")?,
                "error": row.try_get::<Option<&str>, _>("error")?,
                "attempts": row.try_get::<i64, _>("attempts")?,
                "receipt": receipt,
            }))
        })
        .collect()
}

/// A queued document and destination selected for delivery.
#[derive(Debug, Clone)]
pub struct PublicationJob {
    /// Content hash of the queued document.
    pub hash: String,
    /// Destination service address.
    pub destination: String,
    /// Serialized document scheduled for delivery.
    pub body: String,
    /// Publication kind used for routing and prioritization.
    pub kind: String,
    /// Unix timestamp after which delivery cannot start.
    pub expires_at: i64,
}

/// Expires pending jobs and reads the next eight eligible jobs in delivery order.
///
/// # Errors
///
/// Returns storage or result-decoding errors.
pub async fn due_publications(
    pool: &SqlitePool,
    now: i64,
) -> Result<Vec<PublicationJob>, StoreError> {
    query("UPDATE social_publication_job SET state='expired',body='' WHERE expires_at<=? AND state='pending'").bind(now).execute(pool).await?;
    let rows = query_as("SELECT hash,destination,body,kind,expires_at FROM social_publication_job WHERE state='pending' AND expires_at>? AND next_attempt<=? ORDER BY next_attempt,CASE kind WHEN 'reply-route' THEN 0 WHEN 'reply-control' THEN 1 WHEN 'reply-ending' THEN 2 WHEN 'reply-admission' THEN 3 ELSE 4 END,rowid LIMIT 8")
        .bind(now)
        .bind(now)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(
            |(hash, destination, body, kind, expires_at)| PublicationJob {
                hash,
                destination,
                body,
                kind,
                expires_at,
            },
        )
        .collect())
}

/// Records a publication result in a write transaction.
///
/// # Errors
///
/// Returns an error if the write transaction fails.
pub async fn publication_result(
    pool: &SqlitePool,
    job: &PublicationJob,
    error: Option<&str>,
    now: i64,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    publication_result_on(&mut tx, job, error, now).await?;
    tx.commit().await
}

/// Accepts a pending job or schedules its next retry in the caller’s transaction.
///
/// # Errors
///
/// Returns storage errors.
pub async fn publication_result_on(
    tx: &mut Transaction<'_, Sqlite>,
    job: &PublicationJob,
    error: Option<&str>,
    now: i64,
) -> Result<(), StoreError> {
    query("UPDATE social_publication_job SET state=CASE WHEN ? IS NULL THEN 'accepted' ELSE 'pending' END,error=?,attempts=MIN(attempts+1,1000000),next_attempt=?+MIN(3600,5*(1<<MIN(attempts,10))),body=CASE WHEN ? IS NULL THEN '' ELSE body END WHERE hash=? AND destination=? AND state='pending'")
        .bind(error).bind(error).bind(now).bind(error).bind(&job.hash).bind(&job.destination).execute(&mut **tx).await?;
    Ok(())
}

/// Records a receipt for a valid acknowledgement hash.
///
/// # Errors
///
/// Returns an error for an invalid hash or a failed write.
pub async fn publication_receipt_on(
    tx: &mut Transaction<'_, Sqlite>,
    job: &PublicationJob,
    accepted_hash: &str,
) -> Result<(), StoreError> {
    if accepted_hash.len() != 64 || !accepted_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(protocol("Invalid publication acknowledgement hash"));
    }
    let receipt = json!({
        "accepted":true,"service":job.destination,"hash":accepted_hash,
        "expires_at":job.expires_at
    });
    query("UPDATE social_publication_job SET receipt=? WHERE hash=? AND destination=?")
        .bind(receipt.to_string())
        .bind(&job.hash)
        .bind(&job.destination)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Marks the pending job as failed in its own transaction.
///
/// # Errors
///
/// Returns an error if the write transaction fails.
pub async fn fail_publication(
    pool: &SqlitePool,
    job: &PublicationJob,
    reason: &str,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    fail_publication_on(&mut tx, job, reason).await?;
    tx.commit().await
}

/// Marks the pending job as failed and clears its body.
///
/// # Errors
///
/// Returns storage errors.
pub async fn fail_publication_on(
    tx: &mut Transaction<'_, Sqlite>,
    job: &PublicationJob,
    reason: &str,
) -> Result<(), StoreError> {
    query("UPDATE social_publication_job SET state='failed',error=?,body='' WHERE hash=? AND destination=? AND state='pending'")
        .bind(reason).bind(&job.hash).bind(&job.destination).execute(&mut **tx).await?;
    Ok(())
}

/// Deletes expired social cache entries and runs evidence cleanup.
///
/// # Errors
///
/// Returns an error if SQLite cannot complete and commit the cleanup.
pub async fn prune(pool: &SqlitePool, now: i64) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    cleanup_on(&mut tx, now).await?;
    let cutoff = now.saturating_sub(EXPIRY_GRACE_SECONDS);
    query("DELETE FROM social_search WHERE rowid IN (SELECT f.rowid FROM social_document d INDEXED BY social_document_expiry CROSS JOIN social_search f WHERE d.kind='snippet' AND d.expires_at<=? AND f.id MATCH ('\"' || replace(substr(d.id,6),'\"','\"\"') || '\"') AND f.id=d.id)")
        .bind(now).execute(&mut *tx).await?;
    query("DELETE FROM social_revision WHERE expires_at<? AND hash NOT IN (SELECT hash FROM social_document)")
        .bind(cutoff).execute(&mut *tx).await?;
    query("DELETE FROM social_document WHERE expires_at<?")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM social_discovery_source WHERE NOT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=social_discovery_source.post)").execute(&mut *tx).await?;
    query("DELETE FROM social_discovery_conflict WHERE NOT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=social_discovery_conflict.post)").execute(&mut *tx).await?;
    query("DELETE FROM social_publication_job WHERE expires_at<?")
        .bind(cutoff)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    #[tokio::test]
    async fn replacement_keeps_other_reply_admission_identities_pending() {
        let store = Store::open_memory().await.unwrap();
        let mut tx = write_tx(&store.pool).await.unwrap();
        let destinations = [String::from("service")];
        for (hash, mailbox, sender) in [
            ("old", "inbox", "owner"),
            ("other-sender", "inbox", "another-owner"),
            ("other-mailbox", "another-inbox", "owner"),
            ("latest", "inbox", "owner"),
        ] {
            let body = json!({"document": {"mailbox":mailbox,"sender_owner":sender}}).to_string();
            enqueue_on(&mut tx, "reply-admission", hash, &body, &destinations, 60)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        let pending = due_publications(&store.pool, 0).await.unwrap();
        assert_eq!(
            pending.into_iter().map(|job| job.hash).collect::<Vec<_>>(),
            ["other-sender", "other-mailbox", "latest"],
        );
    }
}
