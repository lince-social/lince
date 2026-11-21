//! Agreed lending intervals and readiness-based acceptance.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use nucleus::{Fact, transfer::disclosure::TransferItem};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction};

use super::invalid;
use crate::StoreError;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// A signed accepted lending interval.
pub struct Accepted {
    /// Organ that originated the loan.
    pub origin: String,
    /// Exchange covered by this acceptance.
    pub exchange: String,
    /// Accepted Transfer revision.
    pub revision: u64,
    /// Inclusive start in Unix milliseconds.
    pub from_ms: i64,
    /// Exclusive end in Unix milliseconds.
    pub until_ms: i64,
    /// Fact recording the accepted interval.
    pub agreement_fact: String,
}

/// Read the most recent acceptance for each exchange.
///
/// # Errors
///
/// Returns an error when the saved loan agreements cannot be read.
pub async fn accepted(pool: &SqlitePool, transfer: &str) -> Result<Vec<Accepted>, StoreError> {
    let mut connection = pool.acquire().await?;
    accepted_on(&mut connection, transfer).await
}

/// Read the most recent acceptance for each exchange on the caller's connection.
///
/// # Errors
///
/// Returns an error when the saved loan agreements cannot be read.
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
            revision: row.get::<i64, _>("revision").cast_unsigned(),
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
    let revision = readiness.revision.cast_signed();
    let created_at = now.to_rfc3339();
    let mut seen = BTreeSet::new();
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
            .bind(transfer).bind(&exchange.giver).bind(&exchange.receiver).bind(revision).fetch_one(&mut **tx).await?;
        if count != 2 {
            continue;
        }
        let (from, until) = interval.bounds().map_err(invalid)?;
        sqlx::query("INSERT OR IGNORE INTO transfer_loan_agreement (transfer_uid,exchange_uid,revision,from_ms,until_ms,agreement_fact_uid,created_at) VALUES (?,?,?,?,?,?,?)")
            .bind(transfer).bind(exchange.uid).bind(revision).bind(from).bind(until).bind(&fact.uid).bind(&created_at).execute(&mut **tx).await?;
    }
    Ok(())
}
