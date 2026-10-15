use crate::StoreError;
use nucleus::visibility::{Data, Organ, Policy, SavedPolicy};
use sqlx::{SqliteConnection, SqlitePool};

fn invalid(error: impl ToString) -> StoreError {
    StoreError::Protocol(error.to_string())
}

pub async fn denied_for_person_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<std::collections::HashSet<String>, StoreError> {
    let organ: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_optional(&mut *connection)
    .await?;
    let organ = organ.ok_or_else(|| invalid("Local Organ unavailable"))?;
    let rows: Vec<(String, i64, Option<String>)> = sqlx::query_as(
        "SELECT record_uid,length(CAST(policy_json AS BLOB)),CASE WHEN length(CAST(policy_json AS BLOB))<=? THEN policy_json END FROM data_visibility WHERE data='record' LIMIT 4097",
    )
    .bind(nucleus::visibility::MAX_POLICY_BYTES as i64)
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() > 4096 {
        return Err(invalid("Record audience snapshot exceeds its limit"));
    }
    let mut bytes = 0usize;
    let mut denied = std::collections::HashSet::new();
    for (record, size, raw) in rows {
        bytes = bytes
            .checked_add(size.max(0) as usize)
            .filter(|bytes| *bytes <= 16 * 1024 * 1024)
            .ok_or_else(|| invalid("Record audience snapshot exceeds its byte limit"))?;
        let raw = raw.ok_or_else(|| invalid("Record audience exceeds its byte limit"))?;
        let policy: Policy = serde_json::from_str(&raw).map_err(invalid)?;
        policy.validate_for(Data::Record).map_err(invalid)?;
        if !policy
            .decide_principal(&organ, Some(person), Some(0))
            .allowed
        {
            denied.insert(record);
        }
    }
    Ok(denied)
}

pub async fn decisions_for_person(
    pool: &SqlitePool,
    person: &str,
    data: Data,
) -> Result<std::collections::HashMap<String, nucleus::visibility::Decision>, StoreError> {
    decisions_for_person_on(&mut *pool.acquire().await?, person, data).await
}

pub async fn decisions_for_person_on(
    connection: &mut SqliteConnection,
    person: &str,
    data: Data,
) -> Result<std::collections::HashMap<String, nucleus::visibility::Decision>, StoreError> {
    let organ: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .fetch_optional(&mut *connection)
    .await?;
    let organ = organ.ok_or_else(|| invalid("Local Organ unavailable"))?;
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT record_uid,CASE WHEN length(CAST(policy_json AS BLOB))<=? THEN policy_json END FROM data_visibility WHERE data=? LIMIT 4097",
    )
    .bind(nucleus::visibility::MAX_POLICY_BYTES as i64)
    .bind(data.as_str())
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() > 4096 {
        return Err(invalid("Audience snapshot exceeds its row limit"));
    }
    let mut decisions = std::collections::HashMap::new();
    let mut bytes = 0usize;
    for (record, raw) in rows {
        let raw = raw.ok_or_else(|| invalid("Audience exceeds its byte limit"))?;
        bytes = bytes
            .checked_add(raw.len())
            .filter(|bytes| *bytes <= 16 * 1024 * 1024)
            .ok_or_else(|| invalid("Audience snapshot exceeds its byte limit"))?;
        let policy: Policy = serde_json::from_str(&raw).map_err(invalid)?;
        policy.validate_for(data).map_err(invalid)?;
        decisions.insert(
            record,
            policy.decide_principal(&organ, Some(person), Some(0)),
        );
    }
    Ok(decisions)
}

pub async fn exact_places_for_person_on(
    connection: &mut SqliteConnection,
    person: &str,
    records: &std::collections::BTreeSet<String>,
) -> Result<std::collections::HashSet<String>, StoreError> {
    let decisions = decisions_for_person_on(connection, person, Data::Place).await?;
    let rows: Vec<(String,String)> = sqlx::query_as(
        "SELECT uid,place_uid FROM record WHERE deleted_at IS NULL AND place_uid IS NOT NULL AND uid IN (SELECT value FROM json_each(?))",
    ).bind(serde_json::to_string(records).map_err(invalid)?).fetch_all(connection).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(record, place)| {
            decisions
                .get(&record)
                .is_some_and(|decision| decision.allowed && decision.radius_metres.is_none())
                .then_some(place)
        })
        .collect())
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
        policy.validate_for(data).map_err(invalid)?;
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
    policy.validate_for(data).map_err(invalid)?;
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
            blocked: false,
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
            blocked: contact.trust == "blocked",
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
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT record_uid,data,policy_json FROM data_visibility WHERE data IN ('record','place')",
    )
    .fetch_all(pool)
    .await?;
    let mut denied: std::collections::HashSet<String> = sqlx::query_scalar("SELECT uid FROM record WHERE place_uid IS NOT NULL AND NOT EXISTS(SELECT 1 FROM data_visibility v WHERE v.record_uid=record.uid AND v.data='place')")
        .fetch_all(pool).await?.into_iter().collect();
    for (record, data, raw) in rows {
        let policy: Policy = serde_json::from_str(&raw).map_err(invalid)?;
        policy
            .validate_for(if data == "record" {
                Data::Record
            } else {
                Data::Place
            })
            .map_err(invalid)?;
        let decision = policy.decide(organ, proximity);
        if proximity.is_none()
            || !decision.allowed
            || (data == "place" && decision.radius_metres.is_some())
        {
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
    Ok(ensure_private_for_person_on(connection, record, data, None)
        .await?
        .is_some())
}

pub async fn ensure_private_for_person_on(
    connection: &mut SqliteConnection,
    record: &str,
    data: Data,
    person: Option<&str>,
) -> Result<Option<SavedPolicy>, StoreError> {
    let mut policy = Policy::default();
    if let Some(person) = person
        && crate::people::is_active_on(connection, person).await?
    {
        policy.include.push(nucleus::visibility::Condition {
            persons: vec![person.into()],
            ..Default::default()
        });
    }
    policy.validate_for(data).map_err(invalid)?;
    let changed = sqlx::query("INSERT OR IGNORE INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,NULL,1,?,?)")
        .bind(record).bind(data.as_str()).bind(serde_json::to_string(&policy).map_err(invalid)?)
        .bind(nucleus::execution::now().to_rfc3339()).execute(connection).await?;
    Ok((changed.rows_affected() == 1).then_some(SavedPolicy {
        controller_uid: String::new(),
        revision: 1,
        policy,
    }))
}

pub async fn import(
    pool: &SqlitePool,
    record: &str,
    data: Data,
    saved: &SavedPolicy,
) -> Result<(), StoreError> {
    saved.policy.validate_for(data).map_err(invalid)?;
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
