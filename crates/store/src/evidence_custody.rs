use crate::{
    StoreError,
    sqlx::{self, Sqlite, Transaction},
};
use nucleus::{
    Cause, NewFact, RecordKind,
    trust_analysis::{
        custody::{self, Acknowledgment, Offer, Policy},
        serving::Artifact,
    },
};
use serde_json::{Value, json};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PolicyState {
    pub version: u64,
    pub policy: Policy,
}

pub async fn policy_on(
    tx: &mut Transaction<'_, Sqlite>,
    person: &str,
) -> Result<PolicyState, StoreError> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT version,policy_json FROM evidence_custody_policy WHERE owner_person_uid=?",
    )
    .bind(person)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some((version, json)) => Ok(PolicyState {
            version: version as u64,
            policy: serde_json::from_str(&json).map_err(invalid)?,
        }),
        None => Ok(PolicyState {
            version: 0,
            policy: Policy::default(),
        }),
    }
}

pub(crate) async fn control_on(
    tx: &mut Transaction<'_, Sqlite>,
    person: &str,
    actor: &str,
    head: &str,
    payload: Value,
    now: i64,
    sign: impl Fn(&str) -> Option<String>,
    intent: Option<&str>,
) -> Result<String, StoreError> {
    if !crate::people::is_active_on(tx, person).await? {
        return Err(invalid("Custody requires an active local operator"));
    }
    let local: String = sqlx::query_scalar(
        "SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_one(&mut **tx)
    .await?;
    let record = nucleus::new_uid("r");
    crate::records::create_with_uid_on(tx,&record,crate::records::NewRecord {slug:None,kind:RecordKind::Plain,head,body:"Private custody control. This is not a contribution, payment or physical-delivery confirmation.",quantity:crate::exact::zero()},&local,None).await?;
    if let Some(policy) = crate::data_visibility::ensure_private_for_person_on(
        tx,
        &record,
        nucleus::visibility::Data::Record,
        Some(person),
    )
    .await?
    {
        crate::sync_ops::log_local_tx(
            tx,
            "data_visibility",
            &record,
            "record",
            crate::sync_ops::OpKind::Set,
            Some(serde_json::to_string(&policy).map_err(invalid)?),
        )
        .await?;
    }
    let at =
        chrono::DateTime::from_timestamp(now, 0).ok_or_else(|| invalid("Invalid custody time"))?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: record.clone(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: Some(actor.into()),
            cause: Cause::user_edit(),
            payload: Some(serde_json::to_string(&payload).map_err(invalid)?),
        },
        &crate::facts::last_hash(tx).await?,
        at,
    );
    fact.signature = sign(&fact.hash);
    if fact.signature.is_none() && intent.is_none() {
        return Err(invalid("Custody control requires signed authorization"));
    }
    crate::facts::insert(tx, &fact).await?;
    if let Some(intent) = intent {
        crate::action_intents::link_pending_fact(tx, intent, &fact).await?;
    }
    Ok(record)
}

pub async fn set_policy_on(
    tx: &mut Transaction<'_, Sqlite>,
    person: &str,
    policy: &Policy,
    expected: u64,
    now: i64,
    sign: impl Fn(&str) -> Option<String>,
    intent: Option<&str>,
) -> Result<PolicyState, StoreError> {
    policy.validate().map_err(invalid)?;
    let state = policy_on(tx, person).await?;
    if state.version != expected || expected >= i64::MAX as u64 {
        return Err(invalid("Custody policy changed; review current settings"));
    }
    let body = serde_json::to_string(policy).map_err(invalid)?;
    if body.len() > 64000 {
        return Err(invalid("Custody policy exceeds its byte bound"));
    }
    let version = expected + 1;
    let record = control_on(
        tx,
        person,
        person,
        "Evidence custody policy",
        json!({"kind":"custody_policy","version":version,"policy":policy}),
        now,
        sign,
        intent,
    )
    .await?;
    sqlx::query("INSERT INTO evidence_custody_policy(owner_person_uid,version,record_uid,policy_json) VALUES(?,?,?,?) ON CONFLICT(owner_person_uid) DO UPDATE SET version=excluded.version,record_uid=excluded.record_uid,policy_json=excluded.policy_json").bind(person).bind(version as i64).bind(record).bind(body).execute(&mut **tx).await?;
    Ok(PolicyState {
        version,
        policy: policy.clone(),
    })
}

pub async fn cleanup_on(
    tx: &mut Transaction<'_, Sqlite>,
    now: i64,
) -> Result<(u64, u64), StoreError> {
    crate::evidence_forwarding::cleanup_on(tx, now).await?;
    let payloads=sqlx::query("DELETE FROM evidence_custody_payload WHERE NOT EXISTS(SELECT 1 FROM evidence_custody_commitment c WHERE c.artifact_hash=evidence_custody_payload.artifact_hash AND c.retained_until>?)").bind(now).execute(&mut **tx).await?.rows_affected();
    let controls = sqlx::query("DELETE FROM evidence_custody_commitment WHERE control_until<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?
        .rows_affected();
    Ok((payloads, controls))
}

pub async fn acknowledgment_on(
    tx: &mut Transaction<'_, Sqlite>,
    offer: &Offer,
) -> Result<Option<Acknowledgment>, StoreError> {
    let row: Option<String> = sqlx::query_scalar(
        "SELECT acknowledgment_json FROM evidence_custody_commitment WHERE offer_hash=?",
    )
    .bind(offer.hash().map_err(invalid)?.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|body| serde_json::from_str(&body).map_err(invalid))
        .transpose()
}

pub struct Retain<'a> {
    pub person: &'a str,
    pub expected_policy: u64,
    pub offer: &'a Offer,
    pub artifact: &'a Artifact,
    pub acknowledgment: &'a Acknowledgment,
    pub now: i64,
}

pub async fn retain_on(
    tx: &mut Transaction<'_, Sqlite>,
    input: Retain<'_>,
    sign: impl Fn(&str) -> Option<String>,
) -> Result<Acknowledgment, StoreError> {
    let Retain {
        person,
        expected_policy,
        offer,
        artifact,
        acknowledgment,
        now,
    } = input;
    offer.validate(now).map_err(invalid)?;
    offer.accepts(artifact, now).map_err(invalid)?;
    if !acknowledgment.matches(offer) {
        return Err(invalid(
            "Acknowledgment does not bind the exact custody offer",
        ));
    }
    if let Some(stored) = acknowledgment_on(tx, offer).await? {
        return Ok(stored);
    }
    cleanup_on(tx, now).await?;
    let state = policy_on(tx, person).await?;
    if state.version != expected_policy
        || !state.policy.enabled
        || offer.retained_until > now.saturating_add(i64::from(state.policy.retention_seconds))
    {
        return Err(invalid(
            "Custody policy changed, disabled or rejects retention",
        ));
    }
    let (items,bytes):(i64,i64)=sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(byte_count),0) FROM evidence_custody_commitment WHERE owner_person_uid=? AND retained_until>?").bind(person).bind(now).fetch_one(&mut **tx).await?;
    let (all_items,all_bytes):(i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM evidence_custody_commitment),(SELECT COALESCE(SUM(length(body)),0) FROM evidence_custody_payload)").fetch_one(&mut **tx).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM evidence_custody_payload WHERE artifact_hash=?)",
    )
    .bind(offer.artifact.as_str())
    .fetch_one(&mut **tx)
    .await?;
    if items >= i64::from(state.policy.items)
        || bytes.saturating_add(i64::from(offer.bytes)) > state.policy.storage_bytes as i64
        || all_items >= i64::from(custody::MAX_ITEMS)
        || all_bytes.saturating_add(if exists { 0 } else { i64::from(offer.bytes) })
            > custody::MAX_STORAGE_BYTES as i64
    {
        return Err(invalid(
            "Custody capacity is full; existing commitments are retained",
        ));
    }
    let record = control_on(
        tx,
        person,
        &offer.custodian,
        "Evidence retention acknowledgment",
        json!({"kind":"custody_acknowledgment","acknowledgment":acknowledgment}),
        now,
        sign,
        None,
    )
    .await?;
    sqlx::query("INSERT OR IGNORE INTO evidence_custody_payload(artifact_hash,kind,body,created_at) VALUES(?,?,?,?)").bind(offer.artifact.as_str()).bind(offer.source_kind.as_str()).bind(artifact.bytes().map_err(invalid)?).bind(now).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO evidence_custody_commitment(offer_hash,record_uid,owner_person_uid,sender_organ_uid,artifact_hash,byte_count,retained_until,control_until,offer_json,acknowledgment_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(offer.hash().map_err(invalid)?.as_str()).bind(&record).bind(person).bind(&offer.sender).bind(offer.artifact.as_str()).bind(offer.bytes).bind(offer.retained_until).bind(offer.retained_until.saturating_add(custody::CONTROL_SECONDS)).bind(serde_json::to_string(offer).map_err(invalid)?).bind(serde_json::to_string(acknowledgment).map_err(invalid)?).bind(now).execute(&mut **tx).await?;
    Ok(acknowledgment.clone())
}

pub async fn inventory_on(
    tx: &mut Transaction<'_, Sqlite>,
    person: &str,
    before: Option<i64>,
) -> Result<Vec<Value>, StoreError> {
    let rows:Vec<(i64,String,String,i64,i64,i64)>=sqlx::query_as("SELECT sequence,record_uid,artifact_hash,byte_count,retained_until,control_until FROM evidence_custody_commitment WHERE owner_person_uid=? AND sequence<? ORDER BY sequence DESC LIMIT 33").bind(person).bind(before.unwrap_or(i64::MAX)).fetch_all(&mut **tx).await?;
    Ok(rows.into_iter().map(|(sequence,record,hash,bytes,until,control)|json!({"sequence":sequence,"record":record,"hash":hash,"bytes":bytes,"retained_until":until,"control_until":control})).collect())
}
