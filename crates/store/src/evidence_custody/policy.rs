use nucleus::trust_analysis::custody::Policy;
use serde::Serialize;
use serde_json::{from_str, json, to_string};
use sqlx::{Sqlite, Transaction, query, query_as};

use super::{Control, control_on, invalid};
use crate::StoreError;

const MAX_POLICY_BYTES: usize = 64_000;

/// The operator’s saved custody policy and optimistic revision.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyState {
    /// Stored revision, or zero before saving the policy.
    pub version: u64,
    /// Current settings or defaults when no settings have been saved.
    pub policy: Policy,
}

/// Reads the operator’s custody policy, falling back to defaults when absent.
/// # Errors
/// Returns an error if the stored policy or revision cannot be read or decoded.
pub async fn policy_on(
    tx: &mut Transaction<'_, Sqlite>,
    person: &str,
) -> Result<PolicyState, StoreError> {
    let row: Option<(i64, String)> = query_as(
        "SELECT version,policy_json FROM evidence_custody_policy WHERE owner_person_uid=?",
    )
    .bind(person)
    .fetch_optional(&mut **tx)
    .await?;
    match row {
        Some((version, json)) => Ok(PolicyState {
            version: u64::try_from(version).map_err(invalid)?,
            policy: from_str(&json).map_err(invalid)?,
        }),
        None => Ok(PolicyState {
            version: 0,
            policy: Policy::default(),
        }),
    }
}

/// Saves validated custody settings and records the authorized change.
/// # Errors
/// Returns an error if settings are invalid or stale, authorization is missing, or the transaction cannot write the policy.
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
    let version = expected.saturating_add(1);
    let stored_version = match i64::try_from(version) {
        Ok(version) if state.version == expected => version,
        _ => return Err(invalid("Custody policy changed; review current settings")),
    };
    let body = to_string(policy).map_err(invalid)?;
    if body.len() > MAX_POLICY_BYTES {
        return Err(invalid("Custody policy exceeds its byte bound"));
    }
    let record = control_on(
        tx,
        Control {
            person,
            actor: person,
            head: "Evidence custody policy",
            payload: &json!({"kind":"custody_policy","version":version,"policy":policy}),
            now,
            intent,
        },
        sign,
    )
    .await?;
    query("INSERT INTO evidence_custody_policy(owner_person_uid,version,record_uid,policy_json) VALUES(?,?,?,?) ON CONFLICT(owner_person_uid) DO UPDATE SET version=excluded.version,record_uid=excluded.record_uid,policy_json=excluded.policy_json")
        .bind(person)
        .bind(stored_version)
        .bind(record)
        .bind(body)
        .execute(&mut **tx)
        .await?;
    Ok(PolicyState {
        version,
        policy: policy.clone(),
    })
}
