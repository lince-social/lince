use crate::StoreError;
use nucleus::visibility::{Data, Organ, Policy, SavedPolicy};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub async fn policy(
    pool: &SqlitePool,
    record: &str,
    data: Data,
) -> Result<Option<SavedPolicy>, StoreError> {
    let row: Option<(Option<String>, i64, String)> = sqlx::query_as("SELECT controller_uid,revision,policy_json FROM data_visibility WHERE record_uid=? AND data=?")
        .bind(record).bind(data.as_str()).fetch_optional(pool).await?;
    row.map(|(controller, revision, raw)| {
        let policy: Policy = serde_json::from_str(&raw).map_err(invalid)?;
        policy.validate().map_err(invalid)?;
        Ok(SavedPolicy {
            controller_uid: controller.unwrap_or_default(),
            revision: revision as u64,
            policy,
        })
    })
    .transpose()
}

pub async fn save_on(
    connection: &mut SqliteConnection,
    record: &str,
    data: Data,
    controller: Option<&str>,
    policy: &Policy,
    expected: u64,
) -> Result<u64, StoreError> {
    policy.validate().map_err(invalid)?;
    let revision = expected
        .checked_add(1)
        .filter(|revision| *revision <= i64::MAX as u64)
        .ok_or_else(|| invalid("Visibility revision limit reached"))?;
    let current: Option<(Option<String>, i64)> = sqlx::query_as(
        "SELECT controller_uid,revision FROM data_visibility WHERE record_uid=? AND data=?",
    )
    .bind(record)
    .bind(data.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if current.as_ref().map_or(0, |(_, revision)| *revision as u64) != expected
        || current
            .as_ref()
            .is_some_and(|(owner, _)| owner.as_deref() != controller)
    {
        return Err(invalid(
            "Visibility changed or belongs to another controller; refresh before saving",
        ));
    }
    let changed = sqlx::query("INSERT INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(record_uid,data) DO UPDATE SET revision=excluded.revision,policy_json=excluded.policy_json,updated_at=excluded.updated_at WHERE data_visibility.revision=? AND data_visibility.controller_uid IS excluded.controller_uid")
        .bind(record).bind(data.as_str()).bind(controller).bind(revision as i64)
        .bind(serde_json::to_string(policy).map_err(invalid)?).bind(nucleus::execution::now().to_rfc3339()).bind(expected as i64)
        .execute(connection).await?.rows_affected();
    if changed != 1 {
        return Err(invalid("Visibility changed; refresh before saving"));
    }
    Ok(revision)
}

pub async fn organs(pool: &SqlitePool) -> Result<Vec<Organ>, StoreError> {
    let mut choices = Vec::new();
    if let Some(local) = crate::organs::local(pool).await? {
        choices.push(Organ {
            uid: local.uid,
            label: "This Organ".into(),
            proximity: Some(0),
        });
    }
    for contact in crate::organs::contacts(pool).await? {
        if choices.iter().any(|organ| organ.uid == contact.record_uid) {
            continue;
        }
        choices.push(Organ {
            uid: contact.record_uid,
            label: contact.head,
            proximity: (contact.trust == "known").then_some(contact.proximity),
        });
    }
    Ok(choices)
}

pub async fn proximity(pool: &SqlitePool, organ: &str) -> Result<Option<u32>, StoreError> {
    if organ.is_empty() {
        return Ok(None);
    }
    if crate::organs::local(pool)
        .await?
        .is_some_and(|local| local.uid == organ)
    {
        return Ok(Some(0));
    }
    Ok(crate::organs::contact(pool, organ)
        .await?
        .filter(|contact| contact.trust == "known")
        .map(|contact| contact.proximity))
}

pub async fn denied_records(
    pool: &SqlitePool,
    organ: &str,
) -> Result<std::collections::HashSet<String>, StoreError> {
    if crate::organs::local(pool)
        .await?
        .is_some_and(|local| local.uid == organ)
    {
        return Ok(std::collections::HashSet::new());
    }
    let proximity = proximity(pool, organ).await?;
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT record_uid,policy_json FROM data_visibility WHERE data IN ('record','place')",
    )
    .fetch_all(pool)
    .await?;
    let mut denied: std::collections::HashSet<String> = sqlx::query_scalar("SELECT uid FROM record WHERE place_uid IS NOT NULL AND NOT EXISTS(SELECT 1 FROM data_visibility v WHERE v.record_uid=record.uid AND v.data='place')")
        .fetch_all(pool).await?.into_iter().collect();
    for (record, raw) in rows {
        let policy: Policy = serde_json::from_str(&raw).map_err(invalid)?;
        policy.validate().map_err(invalid)?;
        if proximity.is_none() || !policy.decide(organ, proximity).allowed {
            denied.insert(record);
        }
    }
    Ok(denied)
}

pub async fn ensure_private_on(
    connection: &mut SqliteConnection,
    record: &str,
    data: Data,
) -> Result<bool, StoreError> {
    let changed = sqlx::query("INSERT OR IGNORE INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,NULL,1,?,?)")
        .bind(record).bind(data.as_str()).bind(serde_json::to_string(&Policy::default()).map_err(invalid)?)
        .bind(nucleus::execution::now().to_rfc3339()).execute(connection).await?;
    Ok(changed.rows_affected() == 1)
}

pub async fn import(
    pool: &SqlitePool,
    record: &str,
    data: Data,
    saved: &SavedPolicy,
) -> Result<(), StoreError> {
    saved.policy.validate().map_err(invalid)?;
    if saved.revision == 0 || saved.revision > i64::MAX as u64 || !saved.controller_uid.is_empty() {
        return Err(invalid("Invalid synchronized visibility policy"));
    }
    sqlx::query("INSERT INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,NULL,?,?,?) ON CONFLICT(record_uid,data) DO UPDATE SET revision=excluded.revision,policy_json=excluded.policy_json,updated_at=excluded.updated_at")
        .bind(record).bind(data.as_str()).bind(saved.revision as i64).bind(serde_json::to_string(&saved.policy).map_err(invalid)?)
        .bind(nucleus::execution::now().to_rfc3339()).execute(pool).await?;
    sqlx::query("UPDATE organ_contact SET share_seen_seq=NULL WHERE share_protein IS NOT NULL")
        .execute(pool)
        .await?;
    Ok(())
}
