//! Read signed receipt versions and verify their original Facts.

use nucleus::{
    Fact,
    execution::now,
    fact::verify_chain_step,
    karma::CanonicalHash,
    transfer::receipt::{Command, Document, Scope},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};

use super::{
    invalid,
    request::{lookup, request_json},
};
use crate::{StoreError, facts};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub version: u64,
    pub document: Document,
    pub fact: Fact,
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    uid: &str,
    version: Option<u64>,
) -> Result<Saved, StoreError> {
    let version = version.map(i64::try_from).transpose().map_err(invalid)?;
    let (owner, version, payload, hash, fact_uid): (String, i64, String, String, String) =
        sqlx::query_as(
            "SELECT r.owner_person_uid,v.version,v.document_json,v.document_hash,v.fact_uid
         FROM transfer_receipt_record r
         JOIN transfer_receipt_version v ON v.record_uid=r.record_uid
         JOIN record d ON d.uid=r.record_uid
         WHERE r.record_uid=? AND d.deleted_at IS NULL AND (? IS NULL OR v.version=?)
         ORDER BY v.version DESC LIMIT 1",
        )
        .bind(uid)
        .bind(version)
        .bind(version)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| invalid("Receipt Record or version is unavailable"))?;
    let document = Document::parse(payload.as_bytes(), now().timestamp()).map_err(invalid)?;
    let fact = facts::get_on(connection, &fact_uid)
        .await?
        .ok_or_else(|| invalid("Original receipt Fact is unavailable"))?;
    if fact.record_uid != uid
        || fact.actor_uid.as_deref() != Some(&owner)
        || fact.payload.as_deref() != Some(&payload)
        || !verify_chain_step(&fact)
        || document.hash().map_err(invalid)?.as_str() != hash
        || !fact.delta.is_zero()
    {
        return Err(invalid("Receipt version differs from its original Fact"));
    }
    Ok(Saved {
        record: uid.into(),
        owner,
        version: version as u64,
        document,
        fact,
    })
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Saved, StoreError> {
    get_on(&mut *pool.acquire().await?, uid, None).await
}

pub async fn get_version(pool: &SqlitePool, uid: &str, version: u64) -> Result<Saved, StoreError> {
    if version == 0 {
        return Err(invalid("Choose a positive receipt version"));
    }
    get_on(&mut *pool.acquire().await?, uid, Some(version)).await
}

#[derive(Serialize, sqlx::FromRow)]
pub struct VersionHeader {
    pub version: i64,
    pub hash: String,
    pub saved_at: String,
}

pub async fn versions(
    pool: &SqlitePool,
    uid: &str,
    before: Option<i64>,
) -> Result<Vec<VersionHeader>, StoreError> {
    sqlx::query_as(
        "SELECT v.version,v.document_hash AS hash,v.created_at AS saved_at
         FROM transfer_receipt_version v JOIN record d ON d.uid=v.record_uid
         WHERE v.record_uid=? AND d.deleted_at IS NULL AND (? IS NULL OR v.version<?)
         ORDER BY v.version DESC LIMIT 33",
    )
    .bind(uid)
    .bind(before)
    .bind(before)
    .fetch_all(pool)
    .await
}

pub async fn find(
    pool: &SqlitePool,
    person: &str,
    document: &Document,
) -> Result<Option<Saved>, StoreError> {
    let identity = document.identity().map_err(invalid)?;
    find_identity(pool, person, document.kind(), &identity).await
}

pub async fn find_publication(
    pool: &SqlitePool,
    person: &str,
    scope: &Scope,
) -> Result<Option<Saved>, StoreError> {
    let identity = scope.id().map_err(invalid)?;
    find_identity(pool, person, "publication", &identity).await
}

async fn find_identity(
    pool: &SqlitePool,
    person: &str,
    kind: &str,
    identity: &CanonicalHash,
) -> Result<Option<Saved>, StoreError> {
    let _span = nucleus::performance::Span::new("receipt-identity-read");
    let mut connection = pool.acquire().await?;
    match find_uid_on(&mut connection, person, kind, identity).await? {
        Some(uid) => get_on(&mut connection, &uid, None).await.map(Some),
        None => Ok(None),
    }
}

pub(super) async fn find_uid_on(
    connection: &mut SqliteConnection,
    person: &str,
    kind: &str,
    identity: &CanonicalHash,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT record_uid FROM transfer_receipt_record
         WHERE owner_person_uid=? AND kind=? AND identity_hash=?",
    )
    .bind(person)
    .bind(kind)
    .bind(identity.as_str())
    .fetch_optional(connection)
    .await
}

pub async fn replay(
    pool: &SqlitePool,
    person: &str,
    request_id: &str,
    command: &Command,
) -> Result<Option<Saved>, StoreError> {
    let mut connection = pool.acquire().await?;
    if let Some((previous, uid, version)) = lookup(&mut connection, request_id).await? {
        if previous != request_json(person, command)? {
            return Err(invalid(
                "Receipt request ID was used with different contents",
            ));
        }
        return get_on(&mut connection, &uid, Some(version as u64))
            .await
            .map(Some);
    }
    Ok(None)
}

pub async fn list(pool: &SqlitePool, person: &str) -> Result<Value, StoreError> {
    let rows: Vec<(String, String, String, i64, i64)> = sqlx::query_as(
        "SELECT r.record_uid,d.head,r.kind,r.expires_at,MAX(v.version)
         FROM transfer_receipt_record r JOIN record d ON d.uid=r.record_uid
         JOIN transfer_receipt_version v ON v.record_uid=r.record_uid
         WHERE r.owner_person_uid=? AND d.deleted_at IS NULL
         GROUP BY r.record_uid ORDER BY r.rowid DESC LIMIT 65",
    )
    .bind(person)
    .fetch_all(pool)
    .await?;
    let entries = rows.iter().take(64).map(|(uid, head, kind, expiry, version)| {
        json!({"record":uid,"head":head,"kind":kind,"expires_at":expiry,"version":version})
    }).collect::<Vec<_>>();
    Ok(json!({"entries":entries,"partial":rows.len()>64}))
}
