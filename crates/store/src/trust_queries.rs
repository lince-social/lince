use crate::StoreError;
use chrono::{DateTime, Utc};
use nucleus::{
    Cause, Fact, NewFact, RecordKind,
    trust_analysis::query::{NAMESPACE, Query},
};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, Transaction};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Saved {
    pub record: String,
    pub owner: String,
    pub name: String,
    pub query: Query,
    pub fact: Fact,
}

fn request_json(person: &str, name: &str, query: &Query) -> Result<String, StoreError> {
    query.validate().map_err(invalid)?;
    if name.trim().is_empty() || name.len() > 256 || name.chars().any(char::is_control) {
        return Err(invalid("Choose a query name of 1–256 characters"));
    }
    String::from_utf8(
        nucleus::karma::canonical_json_bytes(&(person, name, query)).map_err(invalid)?,
    )
    .map_err(invalid)
}

pub async fn get_on(connection: &mut SqliteConnection, record: &str) -> Result<Saved, StoreError> {
    let (owner,payload,hash,fact,name):(String,String,String,String,String) = sqlx::query_as("SELECT q.owner_person_uid,q.policy_json,q.policy_hash,q.fact_uid,r.head FROM trust_query_record q JOIN record r ON r.uid=q.record_uid WHERE q.record_uid=? AND r.deleted_at IS NULL")
        .bind(record).fetch_optional(&mut *connection).await?.ok_or_else(||invalid("Saved reader query is unavailable"))?;
    let query: Query = serde_json::from_str(&payload).map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Saved reader query audit Fact is unavailable"))?;
    if query.id().map_err(invalid)?.as_str() != hash
        || fact.record_uid != record
        || fact.actor_uid.as_deref() != Some(&owner)
        || fact.payload.as_deref() != Some(&payload)
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid(
            "Reader query differs from its saved policy and original audit Fact",
        ));
    }
    Ok(Saved {
        record: record.into(),
        owner,
        name,
        query,
        fact,
    })
}

pub async fn replay_on(
    connection: &mut SqliteConnection,
    person: &str,
    id: &str,
    name: &str,
    query: &Query,
) -> Result<Option<Saved>, StoreError> {
    let previous:Option<(String,String,String)> = sqlx::query_as("SELECT owner_person_uid,request_json,record_uid FROM trust_query_request WHERE request_id=?")
        .bind(id).fetch_optional(&mut *connection).await?;
    if let Some((owner, request, record)) = previous {
        if owner != person || request != request_json(person, name, query)? {
            return Err(invalid(
                "Reader query request ID was used with different fields or owner",
            ));
        }
        return get_on(connection, &record).await.map(Some);
    }
    Ok(None)
}

pub async fn find_on(
    connection: &mut SqliteConnection,
    person: &str,
    query: &Query,
) -> Result<Option<Saved>, StoreError> {
    let record: Option<String> = sqlx::query_scalar(
        "SELECT record_uid FROM trust_query_record WHERE owner_person_uid=? AND policy_hash=?",
    )
    .bind(person)
    .bind(query.id().map_err(invalid)?.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    match record {
        Some(record) => get_on(connection, &record).await.map(Some),
        None => Ok(None),
    }
}

pub struct Input {
    pub record: String,
    pub person: String,
    pub request_id: String,
    pub name: String,
    pub query: Query,
    pub authorization_intent: Option<String>,
}

pub struct Commit {
    pub saved: Saved,
    pub created: bool,
}

pub async fn save<F>(
    mut tx: Transaction<'static, Sqlite>,
    input: Input,
    now: DateTime<Utc>,
    sign: F,
) -> Result<(Commit, Transaction<'static, Sqlite>), StoreError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let request = request_json(&input.person, &input.name, &input.query)?;
    if !nucleus::valid_uid(&input.record, "r")
        || !nucleus::valid_uid(&input.person, "r")
        || input.request_id.is_empty()
        || input.request_id.len() > 256
        || input.request_id.chars().any(char::is_control)
    {
        return Err(invalid(
            "Choose canonical owner/Record identities and a request ID of 1–256 characters",
        ));
    }
    if !crate::people::is_active_on(&mut tx, &input.person).await? {
        return Err(invalid("Save a query for an active Person"));
    }
    if let Some(saved) = replay_on(
        &mut tx,
        &input.person,
        &input.request_id,
        &input.name,
        &input.query,
    )
    .await?
    {
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    if let Some(saved) = find_on(&mut tx, &input.person, &input.query).await? {
        sqlx::query("INSERT INTO trust_query_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)")
            .bind(&input.request_id).bind(&input.person).bind(request).bind(&saved.record).execute(&mut *tx).await?;
        return Ok((
            Commit {
                saved,
                created: false,
            },
            tx,
        ));
    }
    let organ: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut *tx)
    .await?;
    crate::records::create_with_uid_on(&mut tx,&input.record,crate::records::NewRecord {
        slug:None,kind:RecordKind::Plain,head:&input.name,body:"A reader-owned evidence search policy. Its weights are local judgments, not a public verdict.",quantity:crate::exact::zero(),
    },&organ,None).await?;
    if let Some(policy) = crate::data_visibility::ensure_private_for_person_on(
        &mut tx,
        &input.record,
        nucleus::visibility::Data::Record,
        Some(&input.person),
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
    let payload = serde_json::to_string(&input.query).map_err(invalid)?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: input.record.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(input.person.clone()),
            cause: Cause::user_edit(),
            payload: Some(payload.clone()),
        },
        &crate::facts::last_hash(&mut tx).await?,
        now,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && input.authorization_intent.is_none() {
        return Err(invalid(
            "Save under the owner's signature or a verified Action intent",
        ));
    }
    crate::facts::insert(&mut tx, &fact).await?;
    if let Some(intent) = &input.authorization_intent {
        crate::action_intents::link_pending_fact(&mut tx, intent, &fact).await?;
    }
    sqlx::query("INSERT INTO trust_query_record(record_uid,owner_person_uid,policy_json,policy_hash,fact_uid,created_at) VALUES(?,?,?,?,?,?)")
        .bind(&input.record).bind(&input.person).bind(&payload).bind(input.query.id().map_err(invalid)?.as_str()).bind(&fact.uid).bind(now.to_rfc3339()).execute(&mut *tx).await?;
    crate::records::set_extension_on(
        &mut tx,
        &input.record,
        NAMESPACE,
        &serde_json::to_value(&input.query).map_err(invalid)?,
    )
    .await?;
    sqlx::query("INSERT INTO trust_query_request(request_id,owner_person_uid,request_json,record_uid) VALUES(?,?,?,?)")
        .bind(&input.request_id).bind(&input.person).bind(request).bind(&input.record).execute(&mut *tx).await?;
    Ok((
        Commit {
            saved: Saved {
                record: input.record,
                owner: input.person,
                name: input.name,
                query: input.query,
                fact,
            },
            created: true,
        },
        tx,
    ))
}
