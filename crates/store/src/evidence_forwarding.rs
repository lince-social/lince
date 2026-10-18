use crate::{
    StoreError,
    sqlx::{self, Sqlite, Transaction},
};
use nucleus::{
    karma::CanonicalHash,
    trust_analysis::custody::{
        Acknowledgment, Offer,
        forwarding::{MAX_ATTEMPTS, MAX_JOBS, SourceMode},
    },
};
use serde_json::{Value, json};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Job {
    pub sequence: i64,
    pub record: String,
    pub owner: String,
    pub source: String,
    pub mode: SourceMode,
    pub policy_version: u64,
    pub offer: Offer,
    pub endpoint: String,
    pub state: String,
    pub attempts: u8,
    pub sent_bytes: u32,
    pub error: Option<String>,
    pub acknowledgment: Option<Acknowledgment>,
}

pub async fn get_on(tx: &mut Transaction<'_, Sqlite>, record: &str) -> Result<Job, StoreError> {
    let row:(i64,String,String,String,i64,String,String,String,i64,i64,Option<String>,Option<String>)=sqlx::query_as("SELECT sequence,owner_person_uid,source_record_uid,source_mode,policy_version,offer_json,endpoint,state,attempts,sent_bytes,error,acknowledgment_json FROM record_evidence_forwarding WHERE record_uid=?").bind(record).fetch_one(&mut **tx).await?;
    Ok(Job {
        sequence: row.0,
        record: record.into(),
        owner: row.1,
        source: row.2,
        mode: match row.3.as_str() {
            "serving" => SourceMode::Serving,
            "custody" => SourceMode::Custody,
            _ => return Err(invalid("Unknown evidence delivery source")),
        },
        policy_version: row.4 as u64,
        offer: serde_json::from_str(&row.5).map_err(invalid)?,
        endpoint: row.6,
        state: row.7,
        attempts: row.8 as u8,
        sent_bytes: row.9 as u32,
        error: row.10,
        acknowledgment: row
            .11
            .map(|body| serde_json::from_str(&body).map_err(invalid))
            .transpose()?,
    })
}

pub async fn cleanup_on(tx: &mut Transaction<'_, Sqlite>, now: i64) -> Result<(), StoreError> {
    sqlx::query("UPDATE record_evidence_forwarding SET state='expired',error='Delivery consent expired' WHERE state IN ('pending','sending','held') AND json_extract(offer_json,'$.deliver_by')<=?").bind(now).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM record_evidence_forwarding WHERE retained_until+2592000<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn interrupted_on(tx: &mut Transaction<'_, Sqlite>, now: i64) -> Result<(), StoreError> {
    sqlx::query("UPDATE record_evidence_forwarding SET state=CASE WHEN attempts>=3 THEN 'failed' ELSE 'pending' END,next_attempt=?,error='Interrupted delivery; no new retention acknowledgment is assumed' WHERE state='sending'").bind(now).execute(&mut **tx).await?;
    Ok(())
}

pub struct Queue<'a> {
    pub owner: &'a str,
    pub source: &'a str,
    pub mode: SourceMode,
    pub policy_version: u64,
    pub offer: &'a Offer,
    pub endpoint: &'a str,
    pub series: &'a CanonicalHash,
    pub control: bool,
    pub now: i64,
}

pub async fn queue_on(
    tx: &mut Transaction<'_, Sqlite>,
    input: Queue<'_>,
    sign: impl Fn(&str) -> Option<String>,
) -> Result<Option<String>, StoreError> {
    let Queue {
        owner,
        source,
        mode,
        policy_version,
        offer,
        endpoint,
        series,
        control,
        now,
    } = input;
    offer.validate(now).map_err(invalid)?;
    let policy = crate::evidence_custody::policy_on(tx, owner).await?;
    if policy.version != policy_version || !policy.policy.forwarding.enabled {
        return Err(invalid("Review current evidence-forwarding policy"));
    }
    cleanup_on(tx, now).await?;
    let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_evidence_forwarding WHERE owner_person_uid=? AND artifact_hash=? AND peer_organ_uid=? AND (policy_version=? OR (state='stored' AND retained_until>?)))").bind(owner).bind(offer.artifact.as_str()).bind(&offer.custodian).bind(policy_version as i64).bind(now).fetch_one(&mut **tx).await?;
    if duplicate {
        return Ok(None);
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM record_evidence_forwarding")
        .fetch_one(&mut **tx)
        .await?;
    if count >= i64::from(nucleus::trust_analysis::custody::MAX_ITEMS) {
        return Err(invalid("Evidence-delivery metadata capacity is full"));
    }
    sqlx::query("UPDATE record_evidence_forwarding SET state='cancelled',error='Superseded by a newly reviewed forwarding policy' WHERE owner_person_uid=? AND artifact_hash=? AND peer_organ_uid=? AND policy_version<>? AND state IN ('pending','sending','held','failed')").bind(owner).bind(offer.artifact.as_str()).bind(&offer.custodian).bind(policy_version as i64).execute(&mut **tx).await?;
    if control {
        sqlx::query("UPDATE record_evidence_forwarding SET state='cancelled',error='Superseded by newer original control evidence' WHERE owner_person_uid=? AND series=? AND peer_organ_uid=? AND control=0 AND state IN ('pending','held')").bind(owner).bind(series.as_str()).bind(&offer.custodian).execute(&mut **tx).await?;
    }
    let mut pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM record_evidence_forwarding WHERE state IN ('pending','sending')",
    )
    .fetch_one(&mut **tx)
    .await?;
    if pending >= MAX_JOBS as i64 && control {
        let removed=sqlx::query("UPDATE record_evidence_forwarding SET state='cancelled',error='Prioritized a correction or withdrawal' WHERE sequence=(SELECT sequence FROM record_evidence_forwarding WHERE state='pending' AND control=0 ORDER BY sequence LIMIT 1)").execute(&mut **tx).await?.rows_affected();
        pending -= removed as i64;
    }
    if pending >= MAX_JOBS as i64 {
        return Err(invalid("Evidence forwarding has eight pending deliveries"));
    }
    let hash = offer.hash().map_err(invalid)?;
    let record=crate::evidence_custody::control_on(tx,owner,&offer.sender,"Reviewed public-evidence forwarding",json!({"kind":"evidence_forwarding","hash":hash,"artifact":offer.artifact,"custodian":offer.custodian,"retained_until":offer.retained_until}),now,sign,None).await?;
    sqlx::query("INSERT INTO record_evidence_forwarding(record_uid,owner_person_uid,source_record_uid,source_mode,policy_version,artifact_hash,series,control,peer_organ_uid,endpoint,offer_hash,offer_json,state,next_attempt,retained_until,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'pending',?,?,?)").bind(&record).bind(owner).bind(source).bind(mode.as_str()).bind(policy_version as i64).bind(offer.artifact.as_str()).bind(series.as_str()).bind(control).bind(&offer.custodian).bind(endpoint).bind(hash.as_str()).bind(serde_json::to_string(offer).map_err(invalid)?).bind(now).bind(offer.retained_until).bind(now).execute(&mut **tx).await?;
    Ok(Some(record))
}

pub async fn pending_on(
    tx: &mut Transaction<'_, Sqlite>,
    now: i64,
) -> Result<Vec<String>, StoreError> {
    sqlx::query_scalar("SELECT record_uid FROM record_evidence_forwarding WHERE state='pending' AND attempts<3 AND next_attempt<=? ORDER BY control DESC,sequence LIMIT 4").bind(now).fetch_all(&mut **tx).await
}

pub async fn claim_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    hash: &CanonicalHash,
    now: i64,
) -> Result<bool, StoreError> {
    Ok(sqlx::query("UPDATE record_evidence_forwarding SET state='sending',attempts=attempts+1,sent_bytes=0,error=NULL WHERE record_uid=? AND offer_hash=? AND state='pending' AND attempts<3 AND next_attempt<=? AND json_extract(offer_json,'$.deliver_by')>?").bind(record).bind(hash.as_str()).bind(now).bind(now).execute(&mut **tx).await?.rows_affected()==1)
}

pub async fn progress_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    hash: &CanonicalHash,
    bytes: u32,
) -> Result<bool, StoreError> {
    Ok(sqlx::query("UPDATE record_evidence_forwarding SET sent_bytes=? WHERE record_uid=? AND offer_hash=? AND state='sending' AND ?>=sent_bytes AND ?<=json_extract(offer_json,'$.bytes')").bind(bytes).bind(record).bind(hash.as_str()).bind(bytes).bind(bytes).execute(&mut **tx).await?.rows_affected()==1)
}

pub async fn result_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    hash: &CanonicalHash,
    result: Result<&Acknowledgment, &str>,
    now: i64,
) -> Result<bool, StoreError> {
    let job = get_on(tx, record).await?;
    if job.offer.hash().map_err(invalid)? != *hash
        || (job.state != "sending" && !(job.state == "cancelled" && result.is_ok()))
    {
        return Ok(false);
    }
    let (state, next, error, ack) = match result {
        Ok(ack) if ack.matches(&job.offer) => (
            "stored",
            now,
            if job.state == "cancelled" {
                Some(
                    "Acknowledged after cancellation; the public copy was already delivered".into(),
                )
            } else {
                None
            },
            Some(serde_json::to_string(ack).map_err(invalid)?),
        ),
        Ok(_) => {
            return Err(invalid(
                "Custodian acknowledgment differs from the reviewed delivery",
            ));
        }
        Err(error) => (
            if job.attempts >= MAX_ATTEMPTS {
                "failed"
            } else {
                "pending"
            },
            now + 5 * i64::from(job.attempts),
            Some(error.chars().take(500).collect::<String>()),
            None,
        ),
    };
    Ok(sqlx::query("UPDATE record_evidence_forwarding SET state=?,next_attempt=?,error=?,acknowledgment_json=? WHERE record_uid=? AND offer_hash=? AND state IN ('sending','cancelled')").bind(state).bind(next).bind(error).bind(ack).bind(record).bind(hash.as_str()).execute(&mut **tx).await?.rows_affected()==1)
}

pub async fn hold_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    error: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE record_evidence_forwarding SET state='held',error=? WHERE record_uid=? AND state IN ('pending','sending')").bind(error.chars().take(500).collect::<String>()).bind(record).execute(&mut **tx).await?;
    Ok(())
}

pub async fn cancel_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    owner: &str,
    hash: &CanonicalHash,
) -> Result<(), StoreError> {
    let job = get_on(tx, record).await?;
    if job.owner != owner || job.offer.hash().map_err(invalid)? != *hash {
        return Err(invalid("Review the current delivery and its exact owner"));
    }
    sqlx::query("UPDATE record_evidence_forwarding SET state='cancelled',error='Cancelled by its operator' WHERE record_uid=? AND state IN ('pending','sending','held','failed')").bind(record).execute(&mut **tx).await?;
    Ok(())
}

pub async fn page_on(
    tx: &mut Transaction<'_, Sqlite>,
    owner: &str,
    before: Option<i64>,
) -> Result<Vec<Value>, StoreError> {
    let rows:Vec<String>=sqlx::query_scalar("SELECT record_uid FROM record_evidence_forwarding WHERE owner_person_uid=? AND sequence<? ORDER BY sequence DESC LIMIT 33").bind(owner).bind(before.unwrap_or(i64::MAX)).fetch_all(&mut **tx).await?;
    let mut result = Vec::new();
    for record in rows {
        result.push(serde_json::to_value(get_on(tx, &record).await?).map_err(invalid)?);
    }
    Ok(result)
}
