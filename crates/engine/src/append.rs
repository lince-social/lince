use chrono::{DateTime, Utc};
use nucleus::{Fact, NewFact};
use store::Store;
use store::sqlx::{Sqlite, Transaction};

use crate::error::EngineError;
use crate::trust::Signer;

pub async fn append_one(
    store: &Store,
    new: NewFact,
    now: DateTime<Utc>,
    signer: Option<&Signer>,
) -> Result<Option<Fact>, EngineError> {
    let mut tx = store::write_tx(&store.pool).await?;
    let fact = append_one_in_transaction(&mut tx, new, now, signer).await?;
    tx.commit().await?;
    Ok(fact)
}

pub(crate) async fn append_one_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    new: NewFact,
    now: DateTime<Utc>,
    signer: Option<&Signer>,
) -> Result<Option<Fact>, EngineError> {
    if let Some(uid) = &new.uid {
        if store::facts::exists(&mut *tx, uid).await? {
            return Ok(None);
        }
    }
    let mut new = new;
    if let Some(signer) = signer {
        new.actor_uid
            .get_or_insert_with(|| signer.actor_uid.clone());
    }
    let prev = store::facts::last_hash(&mut *tx).await?;
    let mut fact = nucleus::fact::seal(new, &prev, now);
    if let Some(signer) = signer {
        if fact.signature.is_none() {
            fact.signature = Some(signer.sign_hash(&fact.hash));
        }
    }
    store::facts::insert(&mut *tx, &fact).await?;
    store::records::bump_quantity(&mut *tx, &fact.record_uid, fact.delta, &now.to_rfc3339())
        .await
        .map_err(|e| match e {
            store::sqlx::Error::RowNotFound => EngineError::UnknownRecord(fact.record_uid.clone()),
            other => EngineError::Store(other),
        })?;
    Ok(Some(fact))
}

pub async fn append_all(
    store: &Store,
    news: Vec<NewFact>,
    now: DateTime<Utc>,
    signer: Option<&Signer>,
) -> Result<Vec<Fact>, EngineError> {
    let mut tx = store::write_tx(&store.pool).await?;
    let mut out: Vec<Fact> = Vec::with_capacity(news.len());
    for new in news {
        if let Some(uid) = &new.uid {
            if store::facts::exists(&mut tx, uid).await? {
                continue;
            }
        }
        let mut new = new;
        if let Some(signer) = signer {
            new.actor_uid
                .get_or_insert_with(|| signer.actor_uid.clone());
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
