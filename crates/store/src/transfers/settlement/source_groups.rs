use nucleus::{
    Cause, Fact, NewFact,
    transfer::{TransferSourceGroupLoserEvidence, TransferSourceGroupResultEvidence},
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, sqlite::SqliteRow};

use super::Signing;
use crate::StoreError;

/// The first fully settled Transfer in a group and its evidence.
#[derive(Debug, Clone)]
pub struct TransferSourceGroupResultRow {
    pub uid: String,
    pub source_uid: String,
    pub policy: String,
    pub winner_transfer_uid: String,
    pub winner_revision: u64,
    pub settlement_uid: String,
    pub fact_uid: String,
    pub created_at: String,
}

/// The evidence that a sibling lost a first-completes group.
#[derive(Debug, Clone)]
pub struct TransferSourceGroupLoserRow {
    pub uid: String,
    pub result_uid: String,
    pub transfer_uid: String,
    pub transfer_revision: u64,
    pub fact_uid: String,
    pub created_at: String,
}

/// The source-group decision as it applies to one Transfer.
#[derive(Debug, Clone)]
pub struct TransferSourceGroupState {
    pub result: TransferSourceGroupResultRow,
    pub loser: Option<TransferSourceGroupLoserRow>,
    pub satiated: bool,
}

fn map_source_group_result(row: SqliteRow) -> TransferSourceGroupResultRow {
    TransferSourceGroupResultRow {
        uid: row.get("uid"),
        source_uid: row.get("source_uid"),
        policy: row.get("policy"),
        winner_transfer_uid: row.get("transfer_uid"),
        winner_revision: row.get::<i64, _>("transfer_revision").cast_unsigned(),
        settlement_uid: row.get("settlement_uid"),
        fact_uid: row.get("fact_uid"),
        created_at: row.get("created_at"),
    }
}

fn map_source_group_loser(row: SqliteRow) -> TransferSourceGroupLoserRow {
    TransferSourceGroupLoserRow {
        uid: row.get("uid"),
        result_uid: row.get("result_uid"),
        transfer_uid: row.get("transfer_uid"),
        transfer_revision: row.get::<i64, _>("transfer_revision").cast_unsigned(),
        fact_uid: row.get("fact_uid"),
        created_at: row.get("created_at"),
    }
}

/// Reads the first-completes decision for a source.
///
/// # Errors
///
/// Returns a storage error if the group result cannot be read.
pub async fn source_group_result(
    pool: &SqlitePool,
    source_uid: &str,
) -> Result<Option<TransferSourceGroupResultRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_source_group_result
         WHERE source_uid = ? AND policy = 'first_completes'",
    )
    .bind(source_uid)
    .fetch_optional(pool)
    .await?
    .map(map_source_group_result))
}

/// Reads the group decision and any loser evidence for this Transfer.
///
/// # Errors
///
/// Returns a storage error if the group state cannot be read.
pub async fn source_group_state_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Option<TransferSourceGroupState>, StoreError> {
    let Some(result_row) = sqlx::query(
        "SELECT result.* FROM transfer transfer
         JOIN transfer_source_group_result result
           ON result.source_uid = transfer.source_uid
          AND result.policy = transfer.satiation
         WHERE transfer.record_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let result = map_source_group_result(result_row);
    let loser = sqlx::query(
        "SELECT * FROM transfer_source_group_loser
         WHERE result_uid = ? AND transfer_uid = ?",
    )
    .bind(&result.uid)
    .bind(transfer_uid)
    .fetch_optional(pool)
    .await?
    .map(map_source_group_loser);
    Ok(Some(TransferSourceGroupState {
        satiated: result.winner_transfer_uid != transfer_uid,
        result,
        loser,
    }))
}

pub(super) async fn record_first_completes_result<F>(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    settlement_uid: &str,
    actor_person_uid: &str,
    signing: &Signing<'_, F>,
) -> Result<(), StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let group =
        sqlx::query("SELECT source_uid, satiation, revision FROM transfer WHERE record_uid = ?")
            .bind(transfer_uid)
            .fetch_one(&mut **tx)
            .await?;
    let Some(source_uid) = group.get::<Option<&str>, _>("source_uid") else {
        return Ok(());
    };
    if group.get::<Option<&str>, _>("satiation") != Some("first_completes") {
        return Ok(());
    }
    let fully_settled: bool = sqlx::query_scalar(
        "SELECT EXISTS(
                 SELECT 1 FROM promise
                 WHERE transfer_uid = ? AND state NOT IN ('open', 'withdrawn')
             )
             AND NOT EXISTS(
                 SELECT 1 FROM promise promise
                 LEFT JOIN transfer_occurrence occurrence
                   ON occurrence.promise_uid = promise.uid
                 WHERE promise.transfer_uid = ?
                   AND promise.state NOT IN ('open', 'withdrawn')
                   AND (occurrence.uid IS NULL OR COALESCE((
                       SELECT SUM(slice.canonical_quantity)
                       FROM transfer_occurrence_settlement_slice slice
                       WHERE slice.occurrence_uid = occurrence.uid
                   ), 0.0) < occurrence.quantity)
             )
             AND NOT EXISTS(
                 SELECT 1 FROM transfer_source_group_result
                 WHERE source_uid = ? AND policy = 'first_completes'
             )",
    )
    .bind(transfer_uid)
    .bind(transfer_uid)
    .bind(source_uid)
    .fetch_one(&mut **tx)
    .await?;
    if !fully_settled {
        return Ok(());
    }
    let winner_revision: i64 = group.get("revision");
    let result_uid = nucleus::new_uid("tsg");
    let evidence = TransferSourceGroupResultEvidence {
        action: "complete-transfer-source-group".into(),
        source_uid: source_uid.into(),
        policy: "first_completes".into(),
        winner_transfer_uid: transfer_uid.into(),
        winner_revision: winner_revision.cast_unsigned(),
        settlement_uid: settlement_uid.into(),
        observed_by_person_uid: actor_person_uid.into(),
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let mut previous_hash = crate::facts::last_hash(tx).await?;
    let result_fact = signing
        .append(
            tx,
            NewFact {
                uid: None,
                record_uid: transfer_uid.into(),
                delta: crate::exact::zero(),
                at: None,
                actor_uid: Some(actor_person_uid.into()),
                cause: Cause::settlement(settlement_uid),
                payload: Some(payload),
            },
            &previous_hash,
            "source-group result requires a signing key or verified Action intent",
        )
        .await?;
    let at = signing.now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_source_group_result
            (uid, source_uid, policy, transfer_uid, transfer_revision,
             settlement_uid, fact_uid, created_at)
         VALUES (?, ?, 'first_completes', ?, ?, ?, ?, ?)",
    )
    .bind(&result_uid)
    .bind(source_uid)
    .bind(transfer_uid)
    .bind(winner_revision)
    .bind(settlement_uid)
    .bind(&result_fact.uid)
    .bind(&at)
    .execute(&mut **tx)
    .await?;
    previous_hash = result_fact.hash;

    let losers = sqlx::query(
        "SELECT record_uid, revision FROM transfer
         WHERE source_uid = ? AND satiation = 'first_completes' AND record_uid != ?
         ORDER BY record_uid",
    )
    .bind(source_uid)
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?;
    for loser in losers {
        let losing_transfer_uid: String = loser.get("record_uid");
        let losing_revision: i64 = loser.get("revision");
        let loser_uid = nucleus::new_uid("tsl");
        let evidence = TransferSourceGroupLoserEvidence {
            action: "satiate-transfer-source-group-sibling".into(),
            source_uid: source_uid.into(),
            policy: "first_completes".into(),
            winner_transfer_uid: transfer_uid.into(),
            losing_transfer_uid: losing_transfer_uid.clone(),
            losing_revision: losing_revision.cast_unsigned(),
            result_uid: result_uid.clone(),
            observed_by_person_uid: actor_person_uid.into(),
        };
        let payload = serde_json::to_string(&evidence)
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        let fact = signing
            .append(
                tx,
                NewFact {
                    uid: None,
                    record_uid: losing_transfer_uid.clone(),
                    delta: crate::exact::zero(),
                    at: None,
                    actor_uid: Some(actor_person_uid.into()),
                    cause: Cause::settlement(settlement_uid),
                    payload: Some(payload),
                },
                &previous_hash,
                "source-group loser evidence requires a signing key or verified Action intent",
            )
            .await?;
        sqlx::query(
            "INSERT INTO transfer_source_group_loser
                (uid, result_uid, transfer_uid, transfer_revision, fact_uid, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(loser_uid)
        .bind(&result_uid)
        .bind(&losing_transfer_uid)
        .bind(losing_revision)
        .bind(&fact.uid)
        .bind(&at)
        .execute(&mut **tx)
        .await?;
        previous_hash = fact.hash;
    }
    Ok(())
}

pub(super) async fn facts_for_settlement(
    pool: &SqlitePool,
    settlement_uid: &str,
) -> Result<Vec<Fact>, StoreError> {
    let fact_uids: Vec<String> = sqlx::query_scalar(
        "SELECT fact_uid FROM (
             SELECT fact_uid, NULL AS sibling FROM transfer_source_group_result
             WHERE settlement_uid = ?1
             UNION ALL
             SELECT loser.fact_uid, loser.transfer_uid FROM transfer_source_group_loser loser
             JOIN transfer_source_group_result result ON result.uid = loser.result_uid
             WHERE result.settlement_uid = ?1
         ) ORDER BY sibling",
    )
    .bind(settlement_uid)
    .fetch_all(pool)
    .await?;
    let mut facts = Vec::with_capacity(fact_uids.len());
    for uid in fact_uids {
        facts.push(
            crate::facts::get(pool, &uid)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?,
        );
    }
    Ok(facts)
}
