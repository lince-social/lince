use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    trust_analysis::serving::{self, Review},
};
use serde_json::Value;
use sqlx::{Sqlite, SqliteConnection, Transaction};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub struct Saved {
    pub record: String,
    pub owner: String,
    pub review: Review,
    pub fact: Fact,
    pub active: bool,
    pub current: bool,
}

pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (owner,source,kind,version,artifact,expires,hash,payload,fact,active,current):(String,String,String,i64,String,i64,String,String,String,bool,bool)=sqlx::query_as("SELECT a.owner_person_uid,a.source_record_uid,a.source_kind,a.source_version,a.artifact_hash,a.expires_at,a.review_hash,a.review_json,a.fact_uid,r.deleted_at IS NULL AND r.kind='plain',NOT EXISTS(SELECT 1 FROM record_evidence_serving n WHERE n.owner_person_uid=a.owner_person_uid AND n.source_record_uid=a.source_record_uid AND n.source_kind=a.source_kind AND n.sequence>a.sequence) FROM record_evidence_serving a JOIN record r ON r.uid=a.record_uid WHERE a.record_uid=?").bind(record).fetch_optional(&mut *connection).await?.ok_or_else(||invalid("Evidence serving permission is unavailable"))?;
    let review: Review = serde_json::from_str(&payload).map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Original serving approval Fact is unavailable"))?;
    if review.person != owner
        || review.request.source.record != source
        || review.request.source.kind.as_str() != kind
        || review.source_version != version as u64
        || review.artifact.hash().map_err(invalid)?.as_str() != artifact
        || review.expires_at != expires
        || review.hash().map_err(invalid)?.as_str() != hash
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(owner.as_str())
        || fact.payload.as_deref() != Some(payload.as_str())
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid(
            "Serving permission differs from its original reviewed approval",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        review,
        fact,
        active,
        current,
    })
}

fn request_json(person: &str, request: &Value) -> Result<String, StoreError> {
    let bytes = nucleus::karma::canonical_json_bytes(&(person, request)).map_err(invalid)?;
    if bytes.len() > 950000 {
        return Err(invalid("Serving request exceeds its byte limit"));
    }
    String::from_utf8(bytes).map_err(invalid)
}

pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    request: &Value,
) -> Result<Option<Saved>, StoreError> {
    let row:Option<(String,String,String)>=sqlx::query_as("SELECT owner_person_uid,request_json,record_uid FROM record_evidence_serving WHERE request_id=?").bind(id).fetch_optional(&mut *connection).await?;
    if let Some((owner, previous, record)) = row {
        if owner != person || previous != request_json(person, request)? {
            return Err(invalid(
                "Serving request ID already has different fields or owner",
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
    let payload = serde_json::to_string(review).map_err(invalid)?;
    let request = request_json(&review.person, &input.request)?;
    let (count,bytes):(i64,i64)=sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(review_json AS BLOB))+length(CAST(request_json AS BLOB))),0) FROM record_evidence_serving").fetch_one(&mut *tx).await?;
    if count >= serving::MAX_PERMISSIONS as i64
        || bytes
            .saturating_add(payload.len() as i64)
            .saturating_add(request.len() as i64)
            > serving::MAX_STORAGE_BYTES as i64
        || payload.len() > 900000
        || !nucleus::valid_uid(&input.record, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
        || !crate::people::is_active_on(&mut tx, &review.person).await?
    {
        return Err(invalid(
            "Serving permission capacity, controller or request limit exceeded",
        ));
    }
    let local: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(&mut tx,&input.record,crate::records::NewRecord {slug:None,kind:RecordKind::Plain,head:"Evidence serving permission",body:"A private approval for a minimal signed document. Its source Record and accounting remain private.",quantity:crate::exact::zero()},&local,None).await?;
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
            "Sign the serving approval or supply its verified client intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    sqlx::query("INSERT INTO record_evidence_serving(record_uid,owner_person_uid,source_record_uid,source_kind,source_version,artifact_hash,expires_at,review_hash,review_json,request_id,request_json,fact_uid,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(&input.record).bind(&review.person).bind(&review.request.source.record).bind(review.request.source.kind.as_str()).bind(review.source_version as i64).bind(review.artifact.hash().map_err(invalid)?.as_str()).bind(review.expires_at).bind(hash.as_str()).bind(&payload).bind(&input.request_id).bind(request).bind(&fact.uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        serving::NAMESPACE,
        &serde_json::to_value(review).map_err(invalid)?,
    )
    .await?;
    let saved = get_on(&mut tx, &input.record).await?;
    Ok((saved, tx))
}
