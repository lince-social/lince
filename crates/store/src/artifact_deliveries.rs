use crate::StoreError;
use nucleus::{Fact, transfer::receipt_delivery::Delivery};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

pub struct Saved {
    pub message: String,
    pub owner: String,
    pub request_json: String,
    pub delivery: Delivery,
    pub fact: Fact,
    pub cancelled: bool,
}

pub async fn get_on(connection: &mut SqliteConnection, message: &str) -> Result<Saved, StoreError> {
    let (owner,request,payload,hash,fact,cancelled):(String,String,String,String,String,bool)=sqlx::query_as("SELECT d.owner_person_uid,d.request_json,d.artifact_json,d.artifact_hash,d.fact_uid,d.cancelled_at IS NOT NULL FROM record_artifact_delivery d JOIN record r ON r.uid=d.message_uid WHERE d.message_uid=? AND r.deleted_at IS NULL").bind(message).fetch_optional(&mut *connection).await?.ok_or_else(||invalid("Outgoing artifact is unavailable"))?;
    let delivery = Delivery::parse(payload.as_bytes(), nucleus::execution::now().timestamp())
        .map_err(invalid)?;
    let fact = crate::facts::get_on(connection, &fact)
        .await?
        .ok_or_else(|| invalid("Delivery Fact is unavailable"))?;
    if delivery.hash().map_err(invalid)?.as_str() != hash
        || fact.actor_uid.as_deref() != Some(owner.as_str())
        || fact
            .payload
            .as_deref()
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()
            .map_err(invalid)?
            .as_ref()
            != Some(&delivery.private_audit(message))
        || !fact.delta.is_zero()
        || !nucleus::fact::verify_chain_step(&fact)
    {
        return Err(invalid("Delivery differs from its original Fact"));
    }
    let private:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND replica_root IS NULL AND kind='plain' AND deleted_at IS NULL)").bind(&fact.record_uid).fetch_one(connection).await?;
    if !private {
        return Err(invalid("The private delivery audit is unavailable"));
    }
    Ok(Saved {
        message: message.into(),
        owner,
        request_json: request,
        delivery,
        fact,
        cancelled,
    })
}

pub async fn get(pool: &SqlitePool, message: &str) -> Result<Saved, StoreError> {
    get_on(&mut *pool.acquire().await?, message).await
}

pub async fn replay(
    pool: &SqlitePool,
    id: &str,
    request_json: &str,
) -> Result<Option<Saved>, StoreError> {
    let previous: Option<(String, String)> = sqlx::query_as(
        "SELECT message_uid,request_json FROM record_artifact_delivery WHERE request_id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match previous {
        Some((message, request)) if request == request_json => get(pool, &message).await.map(Some),
        Some(_) => Err(invalid(
            "Delivery request ID was used with different contents",
        )),
        None => Ok(None),
    }
}

pub async fn messages(pool: &SqlitePool) -> Result<Vec<(String, bool)>, StoreError> {
    sqlx::query_as("SELECT uid,MAX(outgoing) FROM (SELECT message_uid AS uid,1 AS outgoing FROM record_artifact_delivery UNION ALL SELECT e.record_uid AS uid,0 AS outgoing FROM record_extension e,json_each(e.fds,'$.parts') p WHERE e.namespace='lince.message-content' AND json_extract(p.value,'$.mime_type')=?) GROUP BY uid ORDER BY uid").bind(nucleus::transfer::receipt_delivery::MIME).fetch_all(pool).await
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxPosition {
    pub created_at: String,
    pub message: String,
}

#[derive(sqlx::FromRow)]
pub struct InboxEntry {
    pub message: String,
    pub outgoing: bool,
    pub created_at: String,
    pub label: String,
}

pub async fn inbox_page(
    pool: &SqlitePool,
    before: Option<&InboxPosition>,
) -> Result<Vec<InboxEntry>, StoreError> {
    sqlx::query_as("SELECT r.uid AS message,EXISTS(SELECT 1 FROM record_artifact_delivery d WHERE d.message_uid=r.uid) AS outgoing,r.created_at,substr(r.head,1,200) AS label FROM record r WHERE r.deleted_at IS NULL AND r.kind='message' AND (? IS NULL OR r.created_at<? OR (r.created_at=? AND r.uid<?)) AND (EXISTS(SELECT 1 FROM record_artifact_delivery d WHERE d.message_uid=r.uid) OR EXISTS(SELECT 1 FROM record_extension e,json_each(e.fds,'$.parts') p WHERE e.record_uid=r.uid AND e.namespace='lince.message-content' AND json_extract(p.value,'$.mime_type')=?)) ORDER BY r.created_at DESC,r.uid DESC LIMIT 257")
        .bind(before.map(|position|position.created_at.as_str()))
        .bind(before.map(|position|position.created_at.as_str()))
        .bind(before.map(|position|position.created_at.as_str()))
        .bind(before.map(|position|position.message.as_str()))
        .bind(nucleus::transfer::receipt_delivery::MIME)
        .fetch_all(pool).await
}
