use std::fmt::{self, Debug, Formatter};

use nucleus::{
    karma::CanonicalHash,
    trust_analysis::custody::{
        MAX_ITEMS, Offer,
        forwarding::{MAX_JOBS, SourceMode},
    },
};
use serde_json::{json, to_string};
use sqlx::{Sqlite, Transaction, query, query_scalar};

use super::invalid;
use crate::{
    StoreError,
    evidence_custody::{Control, control_on, policy_on},
};

/// Expires delivery consent and removes queue metadata past its retention period.
/// # Errors
/// Returns an error if the expired or retained jobs cannot be updated.
pub async fn cleanup_on(tx: &mut Transaction<'_, Sqlite>, now: i64) -> Result<(), StoreError> {
    query("UPDATE record_evidence_forwarding SET state='expired',error='Delivery consent expired' WHERE state IN ('pending','sending','held') AND json_extract(offer_json,'$.deliver_by')<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    query("DELETE FROM record_evidence_forwarding WHERE retained_until+2592000<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// A reviewed offer and the state needed to admit it into the forwarding queue.
pub struct Queue<'a> {
    /// Operator reviewing the delivery.
    pub owner: &'a str,
    /// Local Record that supplies or identifies the artifact.
    pub source: &'a str,
    /// How the artifact will be obtained.
    pub mode: SourceMode,
    /// Policy revision reviewed for the delivery.
    pub policy_version: u64,
    /// Exact artifact and recipient covered by the offer.
    pub offer: &'a Offer,
    /// Pinned custodian endpoint.
    pub endpoint: &'a str,
    /// Identity tying subsequent evidence revisions and controls together.
    pub series: &'a CanonicalHash,
    /// Whether the offer conveys a correction or withdrawal.
    pub control: bool,
    /// Current Unix time in seconds.
    pub now: i64,
}

impl Debug for Queue<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Queue")
            .field("mode", &self.mode)
            .field("policy_version", &self.policy_version)
            .field("control", &self.control)
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}

/// Admits a reviewed offer, returning the control Record or no Record for a duplicate.
/// # Errors
/// Returns an error if the offer, policy, queue capacity, authorization, or write fails validation.
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
    let policy = policy_on(tx, owner).await?;
    if policy.version != policy_version || !policy.policy.forwarding.enabled {
        return Err(invalid("Review current evidence-forwarding policy"));
    }
    cleanup_on(tx, now).await?;
    let stored_version = i64::try_from(policy_version).map_err(invalid)?;
    let duplicate: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM record_evidence_forwarding WHERE owner_person_uid=? AND artifact_hash=? AND peer_organ_uid=? AND (policy_version=? OR (state='stored' AND retained_until>?)))")
        .bind(owner)
        .bind(offer.artifact.as_str())
        .bind(&offer.custodian)
        .bind(stored_version)
        .bind(now)
        .fetch_one(&mut **tx)
        .await?;
    if duplicate {
        return Ok(None);
    }
    let count: i64 = query_scalar("SELECT COUNT(*) FROM record_evidence_forwarding")
        .fetch_one(&mut **tx)
        .await?;
    if count >= i64::from(MAX_ITEMS) {
        return Err(invalid("Evidence-delivery metadata capacity is full"));
    }
    query("UPDATE record_evidence_forwarding SET state='cancelled',error='Superseded by a newly reviewed forwarding policy' WHERE owner_person_uid=? AND artifact_hash=? AND peer_organ_uid=? AND policy_version<>? AND state IN ('pending','sending','held','failed')")
        .bind(owner)
        .bind(offer.artifact.as_str())
        .bind(&offer.custodian)
        .bind(stored_version)
        .execute(&mut **tx)
        .await?;
    if control {
        query("UPDATE record_evidence_forwarding SET state='cancelled',error='Superseded by newer original control evidence' WHERE owner_person_uid=? AND series=? AND peer_organ_uid=? AND control=0 AND state IN ('pending','held')")
            .bind(owner)
            .bind(series.as_str())
            .bind(&offer.custodian)
            .execute(&mut **tx)
            .await?;
    }
    let mut pending: i64 = query_scalar(
        "SELECT COUNT(*) FROM record_evidence_forwarding WHERE state IN ('pending','sending')",
    )
    .fetch_one(&mut **tx)
    .await?;
    let pending_limit = i64::try_from(MAX_JOBS).map_err(invalid)?;
    if pending >= pending_limit && control {
        let removed = query("UPDATE record_evidence_forwarding SET state='cancelled',error='Prioritized a correction or withdrawal' WHERE sequence=(SELECT sequence FROM record_evidence_forwarding WHERE state='pending' AND control=0 ORDER BY sequence LIMIT 1)")
            .execute(&mut **tx)
            .await?
            .rows_affected();
        pending -= i64::try_from(removed).map_err(invalid)?;
    }
    if pending >= pending_limit {
        return Err(invalid("Evidence forwarding has eight pending deliveries"));
    }
    let hash = offer.hash().map_err(invalid)?;
    let record = control_on(
        tx,
        Control {
            person: owner,
            actor: &offer.sender,
            head: "Reviewed public-evidence forwarding",
            payload: &json!({"kind":"evidence_forwarding","hash":hash,"artifact":offer.artifact,"custodian":offer.custodian,"retained_until":offer.retained_until}),
            now,
            intent: None,
        },
        sign,
    )
    .await?;
    query("INSERT INTO record_evidence_forwarding(record_uid,owner_person_uid,source_record_uid,source_mode,policy_version,artifact_hash,series,control,peer_organ_uid,endpoint,offer_hash,offer_json,state,next_attempt,retained_until,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'pending',?,?,?)")
        .bind(&record)
        .bind(owner)
        .bind(source)
        .bind(mode.as_str())
        .bind(stored_version)
        .bind(offer.artifact.as_str())
        .bind(series.as_str())
        .bind(control)
        .bind(&offer.custodian)
        .bind(endpoint)
        .bind(hash.as_str())
        .bind(to_string(offer).map_err(invalid)?)
        .bind(now)
        .bind(offer.retained_until)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(Some(record))
}
