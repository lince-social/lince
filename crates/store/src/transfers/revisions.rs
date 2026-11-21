use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact,
    transfer::{TransferRevisionEvidence, TransferRevisionSnapshot},
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

pub(super) mod promise;
pub(super) mod snapshot;
pub(super) mod whole_draft;

use snapshot::revision_snapshot;

/// The written revision, a replay of it, or the current revision after a stale request.
#[derive(Debug, Clone)]
pub enum RevisionCommit {
    Committed {
        revision: u64,
        fact: Fact,
        parent_facts: Vec<Fact>,
    },
    Replayed {
        revision: u64,
        fact: Fact,
    },
    Stale {
        current_revision: u64,
    },
}

/// Reads the Transfer, revision number, and action associated with a request key.
///
/// # Errors
///
/// Returns an error if storage fails or the evidence cannot be decoded.
pub async fn revision_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, u64, String)>, StoreError> {
    sqlx::query(
        "SELECT tr.transfer_uid, tr.revision, f.payload
         FROM transfer_revision tr JOIN fact f ON f.uid = tr.fact_uid
         WHERE tr.idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await?
    .map(|row| -> Result<_, StoreError> {
        let payload: &str = row.get("payload");
        let evidence: TransferRevisionEvidence = serde_json::from_str(payload)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        Ok((
            row.get("transfer_uid"),
            row.get::<i64, _>("revision").cast_unsigned(),
            evidence.action,
        ))
    })
    .transpose()
}

pub(super) fn fact_has_transfer_revision_action(fact: &Fact, expected_action: &str) -> bool {
    fact.payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<TransferRevisionEvidence>(payload).ok())
        .is_some_and(|evidence| evidence.action == expected_action)
}

pub(crate) async fn cancel_stale_item_delivery(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
    revision: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE transfer_delivery_outbox SET status = 'cancelled' WHERE status IN ('queued', 'failed') AND transfer_revision < ? AND delivery_uid IN (SELECT uid FROM transfer_delivery_policy WHERE transfer_uid = ?) AND EXISTS (SELECT 1 FROM promise WHERE transfer_uid = ? AND item_json IS NOT NULL)")
        .bind(revision).bind(transfer).bind(transfer).execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn advance_transfer_revision(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    expected_revision: u64,
) -> Result<Result<i64, u64>, StoreError> {
    let expected = i64::try_from(expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let revision = expected
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("transfer revision overflow".into()))?;
    let result =
        sqlx::query("UPDATE transfer SET revision = ? WHERE record_uid = ? AND revision = ?")
            .bind(revision)
            .bind(transfer_uid)
            .bind(expected)
            .execute(&mut **tx)
            .await?;
    if result.rows_affected() != 0 {
        return Ok(Ok(revision));
    }
    let current: i64 = sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
        .bind(transfer_uid)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    Ok(Err(current.cast_unsigned()))
}

pub(crate) struct RevisionChange<'a> {
    pub(crate) revision: i64,
    pub(crate) previous_revision: u64,
    pub(crate) request_id: &'a str,
    pub(crate) action: &'a str,
}

pub(crate) async fn insert_revision_fact<F>(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    change: RevisionChange<'_>,
    actor_uid: Option<String>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let snapshot = revision_snapshot(tx, transfer_uid).await?;
    append_revision(
        tx,
        snapshot,
        RevisionWrite {
            transfer_uid,
            revision: change.revision,
            previous: Some(change.previous_revision),
            request_key: change.request_id,
            action: change.action,
            actor: actor_uid,
            delta: crate::exact::zero(),
            authorization: RevisionAuthorization::Optional,
        },
        now,
        &now.to_rfc3339(),
        sign,
    )
    .await
}

pub(super) enum RevisionAuthorization<'a> {
    Optional,
    Required {
        intent: Option<&'a str>,
        missing: &'static str,
    },
}

pub(super) struct RevisionWrite<'a> {
    pub(super) transfer_uid: &'a str,
    pub(super) revision: i64,
    pub(super) previous: Option<u64>,
    pub(super) request_key: &'a str,
    pub(super) action: &'a str,
    pub(super) actor: Option<String>,
    pub(super) delta: nucleus::DecimalValue,
    pub(super) authorization: RevisionAuthorization<'a>,
}

pub(super) async fn append_revision<F>(
    tx: &mut Transaction<'_, Sqlite>,
    terms: TransferRevisionSnapshot,
    write: RevisionWrite<'_>,
    now: DateTime<Utc>,
    at: &str,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let evidence = TransferRevisionEvidence {
        action: write.action.into(),
        idempotency_key: Some(write.request_key.into()),
        previous_revision: write.previous,
        terms,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| StoreError::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: write.transfer_uid.into(),
            delta: write.delta,
            at: None,
            actor_uid: write.actor,
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    let intent = match write.authorization {
        RevisionAuthorization::Optional => None,
        RevisionAuthorization::Required { intent, missing } => {
            if fact.signature.is_none() && intent.is_none() {
                return Err(StoreError::Protocol(missing.into()));
            }
            intent
        }
    };
    crate::facts::insert(tx, &fact).await?;
    if let Some(intent) = intent {
        crate::action_intents::link_pending_fact(tx, intent, &fact).await?;
    }
    sqlx::query(
        "INSERT INTO transfer_revision
            (transfer_uid, revision, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(write.transfer_uid)
    .bind(write.revision)
    .bind(&fact.uid)
    .bind(write.request_key)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(fact)
}

pub(crate) async fn reset_agreements_for_revision(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    revision: i64,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE transfer_agreement SET level = 0, revision = ?, at = ?
         WHERE transfer_uid = ?",
    )
    .bind(revision)
    .bind(now.to_rfc3339())
    .bind(transfer_uid)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE promise SET state = 'proposed', updated_at = ?
         WHERE transfer_uid = ? AND state = 'agreed'",
    )
    .bind(now.to_rfc3339())
    .bind(transfer_uid)
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE transfer_dependency SET revision = ? WHERE transfer_uid = ?")
        .bind(revision)
        .bind(transfer_uid)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Reads the fact recording the specified Transfer revision.
///
/// # Errors
///
/// Returns an error if the revision exceeds SQLite's range or storage fails.
pub async fn revision_fact(
    pool: &SqlitePool,
    transfer_uid: &str,
    revision: u64,
) -> Result<Option<Fact>, StoreError> {
    let revision = i64::try_from(revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let Some(fact_uid) = sqlx::query_scalar::<_, String>(
        "SELECT fact_uid FROM transfer_revision WHERE transfer_uid = ? AND revision = ?",
    )
    .bind(transfer_uid)
    .bind(revision)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    crate::facts::get(pool, &fact_uid).await
}
