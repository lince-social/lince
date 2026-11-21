//! Inputs for evaluating agreement readiness at the current revision.

use nucleus::transfer::{TransferRevisionDependency, exchange::ExchangeRoute};
use sqlx::{Row, SqliteConnection, SqlitePool};

use super::coalitions::{AgreementCoalitionRow, agreement_coalition_on};
use crate::{
    StoreError,
    transfers::{
        TransferLocationSnapshot, dependencies_of_on,
        item_terms::{decode_item, location_snapshot},
    },
};

/// A party's agreement level at the current revision.
#[derive(Debug, Clone)]
pub struct AgreementPartyLevelRow {
    pub party_uid: String,
    pub person_uid: String,
    pub level: u8,
    pub revision: u64,
}

/// The terms and state of a Promise used in the readiness evaluation.
#[derive(Debug, Clone)]
pub struct AgreementPromiseReadinessRow {
    pub uid: String,
    pub exchange: Option<ExchangeRoute>,
    pub record_uid: Option<String>,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    pub person_uid: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub location: Option<TransferLocationSnapshot>,
    pub state: String,
    pub revision: u64,
}

/// The current Transfer terms, parties, dependencies, and coalition.
#[derive(Debug, Clone)]
pub struct TransferAgreementReadinessInput {
    pub transfer_uid: String,
    pub revision: u64,
    pub agreement_type: String,
    pub agreement_pct: Option<u8>,
    pub parties: Vec<AgreementPartyLevelRow>,
    pub promises: Vec<AgreementPromiseReadinessRow>,
    pub dependencies: Vec<TransferRevisionDependency>,
    pub coalition: Option<AgreementCoalitionRow>,
}

/// Reads the current agreement inputs for a Transfer.
///
/// # Errors
///
/// Returns an error for a missing Transfer, invalid stored terms, or a failed query.
pub async fn agreement_readiness_input(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<TransferAgreementReadinessInput, StoreError> {
    let mut connection = pool.acquire().await?;
    agreement_readiness_input_on(&mut connection, transfer_uid).await
}

pub(crate) async fn agreement_readiness_input_on(
    connection: &mut SqliteConnection,
    transfer_uid: &str,
) -> Result<TransferAgreementReadinessInput, StoreError> {
    let transfer = sqlx::query(
        "SELECT revision, agreement_type, agreement_pct FROM transfer WHERE record_uid = ?",
    )
    .bind(transfer_uid)
    .fetch_optional(&mut *connection)
    .await?
    .ok_or(sqlx::Error::RowNotFound)?;
    let revision: i64 = transfer.get("revision");
    let parties = sqlx::query(
        "SELECT tp.uid AS party_uid, tp.actor_uid AS person_uid,
                coalesce(ta.level, 0) AS level, coalesce(ta.revision, ?1) AS revision
         FROM transfer_party tp
         LEFT JOIN transfer_agreement ta
           ON ta.transfer_uid = tp.transfer_uid AND ta.party_uid = tp.uid
              AND ta.revision = ?1
         WHERE tp.transfer_uid = ?2 ORDER BY tp.uid",
    )
    .bind(revision)
    .bind(transfer_uid)
    .map(|row| AgreementPartyLevelRow {
        party_uid: row.get("party_uid"),
        person_uid: row.get("person_uid"),
        level: row.get::<i64, _>("level") as u8,
        revision: row.get::<i64, _>("revision").cast_unsigned(),
    })
    .fetch_all(&mut *connection)
    .await?;
    let promises = sqlx::query(
        "SELECT uid, record_uid, concept_uid, unit_uid, party_uid, delta,
                window_start, window_end, location_lat, location_lon,
                location_address, state, revision, item_json
         FROM promise WHERE transfer_uid = ? AND state != 'withdrawn'
         ORDER BY uid",
    )
    .bind(transfer_uid)
    .try_map(|row| {
        Ok(AgreementPromiseReadinessRow {
            uid: row.get("uid"),
            exchange: decode_item(row.get("item_json"))?.and_then(|item| item.exchange),
            record_uid: row.get("record_uid"),
            concept_uid: row.get("concept_uid"),
            unit_uid: row.get("unit_uid"),
            person_uid: row.get("party_uid"),
            delta: row.get("delta"),
            window_start: row.get("window_start"),
            window_end: row.get("window_end"),
            location: location_snapshot(
                row.get("location_lat"),
                row.get("location_lon"),
                row.get("location_address"),
            ),
            state: row.get("state"),
            revision: row.get::<i64, _>("revision").cast_unsigned(),
        })
    })
    .fetch_all(&mut *connection)
    .await?;
    let agreement_pct = transfer
        .get::<Option<i64>, _>("agreement_pct")
        .map(|value| {
            u8::try_from(value).map_err(|_| {
                sqlx::Error::Protocol("agreement percentage is outside the u8 range".into())
            })
        })
        .transpose()?;
    if agreement_pct.is_some_and(|value| value > 100) {
        return Err(sqlx::Error::Protocol(
            "agreement percentage must not exceed 100".into(),
        ));
    }
    Ok(TransferAgreementReadinessInput {
        transfer_uid: transfer_uid.into(),
        revision: revision.cast_unsigned(),
        agreement_type: transfer.get("agreement_type"),
        agreement_pct,
        parties,
        promises,
        dependencies: dependencies_of_on(connection, transfer_uid).await?,
        coalition: agreement_coalition_on(connection, transfer_uid, revision.cast_unsigned())
            .await?,
    })
}
