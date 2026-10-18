use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    identity::roots::{self, KeyStatus, Review, State},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Sqlite, SqliteConnection, Transaction};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub revision: u64,
    pub review: Review,
    pub fact: Fact,
    pub active: bool,
}

pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (organ,revision,owner,key,hash,payload,fact,active):(String,i64,String,String,String,String,String,bool)=sqlx::query_as("SELECT a.organ_uid,a.revision,a.owner_person_uid,a.public_key,a.review_hash,a.review_json,a.fact_uid,r.deleted_at IS NULL AND r.kind='plain' FROM organ_root_recognition a JOIN record r ON r.uid=a.record_uid WHERE a.record_uid=?").bind(record).fetch_optional(&mut *connection).await?.ok_or_else(||invalid("Root recognition is unavailable"))?;
    let review: Review = serde_json::from_str(&payload).map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Original root recognition Fact is unavailable"))?;
    if review.proof.organ != organ
        || review.person != owner
        || review.proof.public_key != key
        || review.previous.revision.checked_add(1) != Some(revision as u64)
        || review.hash().map_err(invalid)?.as_str() != hash
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(&owner)
        || fact.payload.as_deref() != Some(&payload)
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid(
            "Root recognition differs from its original reviewed decision and audit Fact",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        revision: revision as u64,
        review,
        fact,
        active,
    })
}

pub async fn current_on(
    connection: &mut SqliteConnection,
    organ: &str,
) -> Result<Option<Saved>, StoreError> {
    let record:Option<String>=sqlx::query_scalar("SELECT record_uid FROM organ_root_recognition WHERE organ_uid=? ORDER BY revision DESC LIMIT 1").bind(organ).fetch_optional(&mut *connection).await?;
    match record {
        Some(record) => get_on(connection, &record).await.map(Some),
        None => Ok(None),
    }
}

pub async fn state_on(connection: &mut SqliteConnection, organ: &str) -> Result<State, StoreError> {
    let original: Option<String> = sqlx::query_scalar(
        "SELECT public_key FROM identity_key WHERE actor_uid=? AND key_id='ed25519:root:v1'",
    )
    .bind(organ)
    .fetch_optional(&mut *connection)
    .await?;
    let saved = current_on(connection, organ).await?;
    let current = saved
        .as_ref()
        .map(|saved| saved.review.proof.public_key.clone())
        .or_else(|| original.clone());
    async fn status(
        connection: &mut SqliteConnection,
        organ: &str,
        key: Option<String>,
    ) -> Result<Option<KeyStatus>, StoreError> {
        if let Some(public_key) = key {
            let revoked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)").bind(organ).bind(&public_key).fetch_one(connection).await?;
            Ok(Some(KeyStatus {
                public_key,
                revoked,
            }))
        } else {
            Ok(None)
        }
    }
    Ok(State {
        original: status(connection, organ, original).await?,
        current: status(connection, organ, current).await?,
        revision: saved.as_ref().map_or(0, |saved| saved.revision),
        available: saved.as_ref().is_none_or(|saved| saved.active),
    })
}

pub async fn history_on(
    connection: &mut SqliteConnection,
    organ: &str,
) -> Result<Vec<Saved>, StoreError> {
    let records:Vec<String>=sqlx::query_scalar("SELECT record_uid FROM organ_root_recognition WHERE organ_uid=? ORDER BY revision LIMIT 65").bind(organ).fetch_all(&mut *connection).await?;
    if records.len() > roots::MAX_ROOTS as usize {
        return Err(invalid("Root recognition history exceeds its limit"));
    }
    let mut history = Vec::new();
    for record in records {
        history.push(get_on(connection, &record).await?);
    }
    Ok(history)
}

fn request_json(person: &str, request: &Value) -> Result<String, StoreError> {
    let bytes = nucleus::karma::canonical_json_bytes(&(person, request)).map_err(invalid)?;
    if bytes.len() > 65536 {
        return Err(invalid("Root recognition request exceeds 64 KiB"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
) -> Result<Option<Saved>, StoreError> {
    let previous:Option<(String,String,String)>=sqlx::query_as("SELECT owner_person_uid,request_json,record_uid FROM organ_root_recognition WHERE request_id=?").bind(id).fetch_optional(&mut *connection).await?;
    if let Some((owner, previous, record)) = previous {
        if owner != person || previous != request_json(person, request)? {
            return Err(invalid(
                "Root recognition request ID already has different fields or owner",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

pub struct Input {
    pub record: String,
    pub request_id: String,
    pub request: Value,
    pub review: Review,
    pub authorization_intent: Option<String>,
}

pub async fn save<F>(
    mut tx: Transaction<'static, Sqlite>,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Saved, Transaction<'static, Sqlite>), StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let review = &input.review;
    let hash = review.hash().map_err(invalid)?;
    if !nucleus::valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
        || !crate::people::is_active_on(&mut tx, &review.person).await?
    {
        return Err(invalid(
            "Choose an active controller, canonical Record and bounded request ID",
        ));
    }
    if state_on(&mut tx, &review.proof.organ).await? != review.previous {
        return Err(invalid("The reviewed root state changed"));
    }
    let payload = serde_json::to_string(review).map_err(invalid)?;
    let local: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(&mut tx,&input.record,crate::records::NewRecord {slug:None,kind:RecordKind::Plain,head:"Reviewed Organ root recognition",body:"An administrator's independent identity decision. Original keys and signed history remain retained.",quantity:crate::exact::zero()},&local,None).await?;
    if let Some(policy) = crate::data_visibility::ensure_private_for_person_on(
        &mut tx,
        &input.record,
        nucleus::visibility::Data::Record,
        Some(&review.person),
    )
    .await?
    {
        crate::sync_ops::log_local_tx(
            &mut tx,
            "data_visibility",
            &input.record,
            "record",
            crate::sync_ops::OpKind::Set,
            Some(serde_json::to_string(&policy).map_err(invalid)?),
        )
        .await?;
    }
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.record.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(review.person.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload.clone()),
        },
        &crate::facts::last_hash(&mut tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Sign the recognition audit or provide its verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    let revision = review.previous.revision + 1;
    sqlx::query("INSERT INTO organ_root_recognition(record_uid,organ_uid,revision,owner_person_uid,public_key,review_hash,review_json,request_id,request_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&input.record).bind(&review.proof.organ).bind(revision as i64).bind(&review.person).bind(&review.proof.public_key).bind(hash.as_str()).bind(&payload).bind(&input.request_id).bind(request_json(&review.person,&input.request)?).bind(&fact.uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        roots::NAMESPACE,
        &serde_json::to_value(review).map_err(invalid)?,
    )
    .await?;
    Ok((
        Saved {
            record: input.record,
            owner: review.person.clone(),
            revision,
            review: input.review,
            fact,
            active: true,
        },
        tx,
    ))
}
