use chrono::{DateTime, Utc};
use nucleus::{Fact, transfer::disclosure::TransferItem};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::StoreError;

mod projection;
pub use projection::{Adjustment, Terms, adjustments, adjustments_on, status, terms, validate_link};

fn invalid(value: impl ToString) -> StoreError {
    StoreError::Protocol(value.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub origin: String,
    pub exchange: String,
    pub revision: u64,
    pub from_ms: i64,
    pub until_ms: i64,
    pub agreement_fact: String,
}

pub async fn accepted(pool: &SqlitePool, transfer: &str) -> Result<Vec<Accepted>, StoreError> {
    let mut connection = pool.acquire().await?;
    accepted_on(&mut connection, transfer).await
}

pub async fn accepted_on(
    connection: &mut SqliteConnection,
    transfer: &str,
) -> Result<Vec<Accepted>, StoreError> {
    let rows = sqlx::query("SELECT a.*,r.organ_uid FROM transfer_loan_agreement a JOIN record r ON r.uid = a.transfer_uid WHERE a.transfer_uid = ? AND a.revision = (SELECT MAX(b.revision) FROM transfer_loan_agreement b WHERE b.transfer_uid = a.transfer_uid AND b.exchange_uid = a.exchange_uid) ORDER BY a.exchange_uid")
        .bind(transfer).fetch_all(connection).await?;
    Ok(rows
        .into_iter()
        .map(|row| Accepted {
            origin: row.get("organ_uid"),
            exchange: row.get("exchange_uid"),
            revision: row.get::<i64, _>("revision") as u64,
            from_ms: row.get("from_ms"),
            until_ms: row.get("until_ms"),
            agreement_fact: row.get("agreement_fact_uid"),
        })
        .collect())
}

pub(crate) async fn accept_ready_on(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
    fact: &Fact,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    let rows: Vec<(String,String)> = sqlx::query_as("SELECT uid,item_json FROM promise WHERE transfer_uid = ? AND json_extract(item_json,'$.loan') IS NOT NULL ORDER BY uid")
        .bind(transfer).fetch_all(&mut **tx).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let readiness = crate::transfer_agreement::read_on(tx, transfer).await?.0;
    let mut seen = std::collections::BTreeSet::new();
    for (promise, raw) in rows {
        let item: TransferItem = serde_json::from_str(&raw).map_err(invalid)?;
        let (Some(interval), Some(exchange)) = (item.loan, item.exchange) else {
            continue;
        };
        if !readiness
            .promises
            .get(&promise)
            .is_some_and(|promise| promise.ready)
            || !seen.insert(exchange.uid.clone())
        {
            continue;
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT p.actor_uid) FROM transfer_party p JOIN transfer_agreement a ON a.party_uid = p.uid AND a.transfer_uid = p.transfer_uid WHERE p.transfer_uid = ? AND p.actor_uid IN (?,?) AND a.revision = ? AND a.level = 2")
            .bind(transfer).bind(&exchange.giver).bind(&exchange.receiver).bind(readiness.revision as i64).fetch_one(&mut **tx).await?;
        if count != 2 {
            continue;
        }
        let (from, until) = interval.bounds().map_err(invalid)?;
        sqlx::query("INSERT OR IGNORE INTO transfer_loan_agreement (transfer_uid,exchange_uid,revision,from_ms,until_ms,agreement_fact_uid,created_at) VALUES (?,?,?,?,?,?,?)")
            .bind(transfer).bind(exchange.uid).bind(readiness.revision as i64).bind(from).bind(until).bind(&fact.uid).bind(now.to_rfc3339()).execute(&mut **tx).await?;
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Extension {
    pub transfer: String,
    pub exchange: String,
    pub until: String,
    pub expected_revision: u64,
    pub person: String,
    pub request_id: String,
    #[serde(skip)]
    pub authorization_intent_uid: Option<String>,
}

pub struct Outcome {
    pub uid: String,
    pub fact: Option<Fact>,
}

pub async fn extend<F>(
    pool: &SqlitePool,
    input: Extension,
    now: DateTime<Utc>,
    sign: F,
) -> Result<Outcome, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    if input.request_id.trim().is_empty()
        || input.request_id.len() > 200
        || input.expected_revision >= i64::MAX as u64
    {
        return Err(invalid(
            "an extension needs a current revision and request identifier",
        ));
    }
    let payload = serde_json::to_string(&input).map_err(invalid)?;
    if let Some((previous, fact)) = sqlx::query_as::<_, (String, String)>(
        "SELECT payload,fact_uid FROM transfer_loan_extension WHERE request_id = ?",
    )
    .bind(&input.request_id)
    .fetch_optional(pool)
    .await?
    {
        if previous != payload {
            return Err(invalid("extension request reused with different terms"));
        }
        return Ok(Outcome {
            uid: fact,
            fact: None,
        });
    }
    crate::transfers::ensure_request_not_used_by_transfer_revision(pool, &input.request_id).await?;
    crate::transfers::ensure_request_not_used_by_invitation_event(pool, &input.request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let authority: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer t JOIN record r ON r.uid = t.record_uid JOIN record own ON own.uid = r.organ_uid AND own.slug = 'local-organ' JOIN transfer_party p ON p.transfer_uid = t.record_uid WHERE t.record_uid = ? AND t.revision = ? AND p.actor_uid = ?)")
        .bind(&input.transfer).bind(input.expected_revision as i64).bind(&input.person).fetch_one(&mut *tx).await?;
    if !authority {
        return Err(invalid(
            "extend the current loan as a participant on its origin",
        ));
    }
    let previous = accepted_on(&mut tx, &input.transfer)
        .await?
        .into_iter()
        .find(|loan| loan.exchange == input.exchange)
        .ok_or_else(|| invalid("agree to the original loan before proposing an extension"))?;
    let rows: Vec<(String,String)> = sqlx::query_as("SELECT uid,item_json FROM promise WHERE transfer_uid = ? AND json_extract(item_json,'$.exchange.uid') = ? ORDER BY uid")
        .bind(&input.transfer).bind(&input.exchange).fetch_all(&mut *tx).await?;
    if rows.is_empty() {
        return Err(invalid("the original loan exchange is unavailable"));
    }
    let mut items = Vec::new();
    for (promise, raw) in rows {
        let mut item: TransferItem = serde_json::from_str(&raw).map_err(invalid)?;
        let loan = item
            .loan
            .as_mut()
            .ok_or_else(|| invalid("this exchange is not a loan"))?;
        loan.until = input.until.clone();
        let (from, until) = loan.bounds().map_err(invalid)?;
        if from != previous.from_ms || until <= previous.until_ms {
            return Err(invalid(
                "an extension must keep the start and move the accepted end later",
            ));
        }
        items.push((promise, serde_json::to_string(&item).map_err(invalid)?));
    }
    let revision = crate::transfers::advance_transfer_revision(
        &mut tx,
        &input.transfer,
        input.expected_revision,
    )
    .await?
    .map_err(|_| invalid("the loan changed after review"))?;
    for (promise, item) in items {
        sqlx::query("UPDATE promise SET item_json = ?,revision = ?,updated_at = ? WHERE uid = ?")
            .bind(item)
            .bind(revision)
            .bind(now.to_rfc3339())
            .bind(promise)
            .execute(&mut *tx)
            .await?;
    }
    crate::transfers::reset_agreements_for_revision(&mut tx, &input.transfer, revision, now)
        .await?;
    let fact = crate::transfers::insert_revision_fact(
        &mut tx,
        &input.transfer,
        revision,
        input.expected_revision,
        &input.request_id,
        "propose-transfer-loan-extension",
        Some(input.person),
        now,
        &sign,
    )
    .await?;
    if let Some(intent) = input.authorization_intent_uid {
        crate::action_intents::link_pending_fact(&mut tx, &intent, &fact).await?;
    } else if fact.signature.is_none() {
        return Err(invalid("an extension needs the participant's signature"));
    }
    sqlx::query("INSERT INTO transfer_loan_extension (request_id,payload,revision,fact_uid) VALUES (?,?,?,?)")
        .bind(input.request_id).bind(payload).bind(revision).bind(&fact.uid).execute(&mut *tx).await?;
    crate::records::bump_quantity(
        &mut tx,
        &input.transfer,
        crate::exact::zero(),
        &now.to_rfc3339(),
    )
    .await?;
    crate::transfers::cancel_stale_item_delivery(&mut tx, &input.transfer, revision).await?;
    tx.commit().await?;
    Ok(Outcome {
        uid: fact.uid.clone(),
        fact: Some(fact),
    })
}
