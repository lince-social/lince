//! The one write path (blueprint 0.3). Appends the fact and bumps the quantity
//! cache in the same transaction. Idempotent: a fact whose uid already exists
//! is a no-op success (sync replay safety).

use chrono::{DateTime, Utc};
use nucleus::{Fact, NewFact};
use store::Store;

use crate::error::EngineError;
use crate::trust::Signer;

pub async fn append_one(
    store: &Store,
    new: NewFact,
    now: DateTime<Utc>,
    signer: Option<&Signer>,
) -> Result<Option<Fact>, EngineError> {
    let mut tx = store.pool.begin().await?;
    if let Some(uid) = &new.uid {
        if store::facts::exists(&mut tx, uid).await? {
            tx.rollback().await?;
            return Ok(None);
        }
    }
    let mut new = new;
    if let Some(signer) = signer {
        // the Cell's identity authors what nobody else claimed
        new.actor_uid.get_or_insert_with(|| signer.actor_uid.clone());
    }
    let prev = store::facts::last_hash(&mut tx).await?;
    let mut fact = nucleus::fact::seal(new, &prev, now);
    if let Some(signer) = signer {
        // imported facts keep their origin signature; local ones get ours
        if fact.signature.is_none() {
            fact.signature = Some(signer.sign_hash(&fact.hash));
        }
    }
    store::facts::insert(&mut tx, &fact).await?;
    store::records::bump_quantity(&mut tx, &fact.record_uid, fact.delta, &now.to_rfc3339())
        .await
        .map_err(|e| match e {
            store::sqlx::Error::RowNotFound => EngineError::UnknownRecord(fact.record_uid.clone()),
            other => EngineError::Store(other),
        })?;
    tx.commit().await?;
    Ok(Some(fact))
}

/// Batch append in one transaction (settlements, sync imports).
pub async fn append_all(
    store: &Store,
    news: Vec<NewFact>,
    now: DateTime<Utc>,
    signer: Option<&Signer>,
) -> Result<Vec<Fact>, EngineError> {
    let mut tx = store.pool.begin().await?;
    let mut out: Vec<Fact> = Vec::with_capacity(news.len());
    for new in news {
        if let Some(uid) = &new.uid {
            if store::facts::exists(&mut tx, uid).await? {
                continue;
            }
        }
        let mut new = new;
        if let Some(signer) = signer {
            new.actor_uid.get_or_insert_with(|| signer.actor_uid.clone());
        }
        let prev = match out.last() {
            Some(f) => f.hash.clone(),
            None => store::facts::last_hash(&mut tx).await?,
        };
        let mut fact = nucleus::fact::seal(new, &prev, now);
        if let Some(signer) = signer {
            if fact.signature.is_none() {
                fact.signature = Some(signer.sign_hash(&fact.hash));
            }
        }
        store::facts::insert(&mut tx, &fact).await?;
        store::records::bump_quantity(&mut tx, &fact.record_uid, fact.delta, &now.to_rfc3339())
            .await?;
        out.push(fact);
    }
    tx.commit().await?;
    Ok(out)
}
