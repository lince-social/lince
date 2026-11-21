//! Stored visibility policy, audience decisions, and Organ proximity.

use std::collections::{BTreeSet, HashMap, HashSet};

use nucleus::{
    execution::now,
    visibility::{Condition, Data, Decision, MAX_POLICY_BYTES, Organ, Policy, SavedPolicy},
};
use serde_json::{from_str, to_string};
use sqlx::{SqliteConnection, SqlitePool, query, query_as, query_scalar};

use crate::{
    StoreError,
    organs::{self as organ_store, LOCAL_ORGAN_SLUG},
    people::is_active_on,
};

const MAX_AUDIENCE_ROWS: usize = 4096;
const MAX_AUDIENCE_BYTES: usize = 16 * 1024 * 1024;

fn invalid(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}

fn encode(policy: &Policy) -> Result<String, StoreError> {
    to_string(policy).map_err(|error| invalid(error.to_string()))
}

fn decode(raw: &str, data: Data) -> Result<Policy, StoreError> {
    let policy: Policy = from_str(raw).map_err(|error| invalid(error.to_string()))?;
    policy.validate_for(data).map_err(invalid)?;
    Ok(policy)
}

async fn local_organ_uid(connection: &mut SqliteConnection) -> Result<String, StoreError> {
    query_scalar("SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL")
        .bind(LOCAL_ORGAN_SLUG)
        .fetch_optional(connection)
        .await?
        .ok_or_else(|| invalid("Local Organ unavailable"))
}

fn extend_budget(bytes: usize, added: usize) -> Option<usize> {
    bytes
        .checked_add(added)
        .filter(|bytes| *bytes <= MAX_AUDIENCE_BYTES)
}

/// Returns records denied to a person by their stored record audience policies.
///
/// # Errors
/// Returns an error if the local Organ is missing or the bounded audience snapshot cannot be read.
pub async fn denied_for_person_on(
    connection: &mut SqliteConnection,
    person: &str,
) -> Result<HashSet<String>, StoreError> {
    let organ = local_organ_uid(connection).await?;
    let rows: Vec<(String, i64, Option<String>)> = query_as(
        "SELECT record_uid,length(CAST(policy_json AS BLOB)),CASE WHEN length(CAST(policy_json AS BLOB))<=? THEN policy_json END FROM data_visibility WHERE data='record' LIMIT ?",
    )
    .bind(i64::try_from(MAX_POLICY_BYTES).map_err(|error| invalid(error.to_string()))?)
    .bind(i64::try_from(MAX_AUDIENCE_ROWS + 1).map_err(|error| invalid(error.to_string()))?)
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() > MAX_AUDIENCE_ROWS {
        return Err(invalid("Record audience snapshot exceeds its limit"));
    }
    let mut bytes = 0usize;
    let mut denied = HashSet::new();
    for (record, size, raw) in rows {
        bytes = usize::try_from(size.max(0))
            .ok()
            .and_then(|size| extend_budget(bytes, size))
            .ok_or_else(|| invalid("Record audience snapshot exceeds its byte limit"))?;
        let raw = raw.ok_or_else(|| invalid("Record audience exceeds its byte limit"))?;
        let policy = decode(&raw, Data::Record)?;
        if !policy
            .decide_principal(&organ, Some(person), Some(0))
            .allowed
        {
            denied.insert(record);
        }
    }
    Ok(denied)
}

/// Reads the person's decisions for all stored policies of the requested data kind.
///
/// # Errors
/// Returns an error if the local Organ is missing or the bounded snapshot cannot be read.
pub async fn decisions_for_person(
    pool: &SqlitePool,
    person: &str,
    data: Data,
) -> Result<HashMap<String, Decision>, StoreError> {
    decisions_for_person_on(&mut *pool.acquire().await?, person, data).await
}

/// Computes person audience decisions using the caller's connection.
///
/// # Errors
/// Returns an error if the local Organ is missing or the bounded snapshot cannot be read.
pub async fn decisions_for_person_on(
    connection: &mut SqliteConnection,
    person: &str,
    data: Data,
) -> Result<HashMap<String, Decision>, StoreError> {
    let organ = local_organ_uid(connection).await?;
    let rows: Vec<(String, Option<String>)> = query_as(
        "SELECT record_uid,CASE WHEN length(CAST(policy_json AS BLOB))<=? THEN policy_json END FROM data_visibility WHERE data=? LIMIT ?",
    )
    .bind(i64::try_from(MAX_POLICY_BYTES).map_err(|error| invalid(error.to_string()))?)
    .bind(data.as_str())
    .bind(i64::try_from(MAX_AUDIENCE_ROWS + 1).map_err(|error| invalid(error.to_string()))?)
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() > MAX_AUDIENCE_ROWS {
        return Err(invalid("Audience snapshot exceeds its row limit"));
    }
    let mut decisions = HashMap::new();
    let mut bytes = 0usize;
    for (record, raw) in rows {
        let raw = raw.ok_or_else(|| invalid("Audience exceeds its byte limit"))?;
        bytes = extend_budget(bytes, raw.len())
            .ok_or_else(|| invalid("Audience snapshot exceeds its byte limit"))?;
        let policy = decode(&raw, data)?;
        decisions.insert(
            record,
            policy.decide_principal(&organ, Some(person), Some(0)),
        );
    }
    Ok(decisions)
}

/// Returns place identities the person may read at exact precision on the selected records.
///
/// # Errors
/// Returns an error if the decisions, selected records, or places cannot be read.
pub async fn exact_places_for_person_on(
    connection: &mut SqliteConnection,
    person: &str,
    records: &BTreeSet<String>,
) -> Result<HashSet<String>, StoreError> {
    let decisions = decisions_for_person_on(connection, person, Data::Place).await?;
    let rows: Vec<(String,String)> = query_as(
        "SELECT uid,place_uid FROM record WHERE deleted_at IS NULL AND place_uid IS NOT NULL AND uid IN (SELECT value FROM json_each(?))",
    ).bind(to_string(records).map_err(|error| invalid(error.to_string()))?).fetch_all(connection).await?;
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

/// Reads and validates one stored visibility policy if it exists.
///
/// # Errors
/// Returns an error for a failed read or malformed policy or revision.
pub async fn policy(
    pool: &SqlitePool,
    record: &str,
    data: Data,
) -> Result<Option<SavedPolicy>, StoreError> {
    let row: Option<(Option<String>, i64, String)> = query_as("SELECT controller_uid,revision,policy_json FROM data_visibility WHERE record_uid=? AND data=?")
        .bind(record).bind(data.as_str()).fetch_optional(pool).await?;
    row.map(|(controller, revision, raw)| {
        let policy = decode(&raw, data)?;
        Ok(SavedPolicy {
            controller_uid: controller.unwrap_or_default(),
            revision: u64::try_from(revision)
                .map_err(|error| invalid(format!("Invalid visibility revision: {error}")))?,
            policy,
        })
    })
    .transpose()
}

/// Saves an edited visibility policy only if its controller and revision still match.
///
/// # Errors
/// Returns an error for invalid policies, stale edits, conflicting controllers, or failed writes.
pub async fn save_on(
    connection: &mut SqliteConnection,
    record: &str,
    data: Data,
    controller: Option<&str>,
    policy: &Policy,
    expected: u64,
) -> Result<u64, StoreError> {
    policy.validate_for(data).map_err(invalid)?;
    let exhausted = || invalid("Visibility revision limit reached");
    let revision = expected.checked_add(1).ok_or_else(exhausted)?;
    let stored_revision = i64::try_from(revision).ok().ok_or_else(exhausted)?;
    let stored_expected = stored_revision - 1;
    let current: Option<(Option<String>, i64)> = query_as(
        "SELECT controller_uid,revision FROM data_visibility WHERE record_uid=? AND data=?",
    )
    .bind(record)
    .bind(data.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    if current.as_ref().map_or(0, |(_, revision)| *revision) != stored_expected
        || current
            .as_ref()
            .is_some_and(|(owner, _)| owner.as_deref() != controller)
    {
        return Err(invalid(
            "Visibility changed or belongs to another controller; refresh before saving",
        ));
    }
    let changed = query("INSERT INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(record_uid,data) DO UPDATE SET revision=excluded.revision,policy_json=excluded.policy_json,updated_at=excluded.updated_at WHERE data_visibility.revision=? AND data_visibility.controller_uid IS excluded.controller_uid")
        .bind(record).bind(data.as_str()).bind(controller).bind(stored_revision)
        .bind(encode(policy)?).bind(now().to_rfc3339()).bind(stored_expected)
        .execute(connection).await?.rows_affected();
    if changed != 1 {
        return Err(invalid("Visibility changed; refresh before saving"));
    }
    Ok(revision)
}

/// Lists the local Organ and its contacts with trust and proximity information.
///
/// # Errors
/// Returns an error if the local Organ or its contacts cannot be read.
pub async fn organs(pool: &SqlitePool) -> Result<Vec<Organ>, StoreError> {
    let mut choices = Vec::new();
    if let Some(local) = organ_store::local(pool).await? {
        choices.push(Organ {
            uid: local.uid,
            label: "This Organ".into(),
            proximity: Some(0),
            blocked: false,
        });
    }
    for contact in organ_store::contacts(pool).await? {
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

/// Reads the proximity of a known contact, using zero for the local Organ.
///
/// # Errors
/// Returns an error if the Organ or contact lookup fails.
pub async fn proximity(pool: &SqlitePool, organ: &str) -> Result<Option<u32>, StoreError> {
    if organ.is_empty() {
        return Ok(None);
    }
    if organ_store::local(pool)
        .await?
        .is_some_and(|local| local.uid == organ)
    {
        return Ok(Some(0));
    }
    Ok(organ_store::contact(pool, organ)
        .await?
        .filter(|contact| contact.trust == "known")
        .map(|contact| contact.proximity))
}

/// Returns records and precise places blocked from an Organ by stored policies and defaults.
///
/// # Errors
/// Returns an error if Organ trust, place data, or stored policies cannot be read.
pub async fn denied_records(pool: &SqlitePool, organ: &str) -> Result<HashSet<String>, StoreError> {
    if organ_store::local(pool)
        .await?
        .is_some_and(|local| local.uid == organ)
    {
        return Ok(HashSet::new());
    }
    let proximity = proximity(pool, organ).await?;
    let rows: Vec<(String, String, String)> = query_as(
        "SELECT record_uid,data,policy_json FROM data_visibility WHERE data IN ('record','place')",
    )
    .fetch_all(pool)
    .await?;
    let mut denied: HashSet<String> = query_scalar("SELECT uid FROM record WHERE place_uid IS NOT NULL AND NOT EXISTS(SELECT 1 FROM data_visibility v WHERE v.record_uid=record.uid AND v.data='place')")
        .fetch_all(pool).await?.into_iter().collect();
    for (record, data, raw) in rows {
        let policy = decode(
            &raw,
            if data == "record" {
                Data::Record
            } else {
                Data::Place
            },
        )?;
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

/// Inserts a private policy for the given data kind if one is absent.
///
/// # Errors
/// Returns an error if the policy cannot be validated, encoded, or saved.
pub async fn ensure_private_on(
    connection: &mut SqliteConnection,
    record: &str,
    data: Data,
) -> Result<bool, StoreError> {
    Ok(ensure_private_for_person_on(connection, record, data, None)
        .await?
        .is_some())
}

/// Inserts a private policy granting access to the optional active person when supplied.
///
/// # Errors
/// Returns an error if person lookup, policy validation, encoding, or insertion fails.
pub async fn ensure_private_for_person_on(
    connection: &mut SqliteConnection,
    record: &str,
    data: Data,
    person: Option<&str>,
) -> Result<Option<SavedPolicy>, StoreError> {
    let mut policy = Policy::default();
    if let Some(person) = person
        && is_active_on(connection, person).await?
    {
        policy.include.push(Condition {
            persons: vec![person.into()],
            ..Default::default()
        });
    }
    policy.validate_for(data).map_err(invalid)?;
    let changed = query("INSERT OR IGNORE INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,NULL,1,?,?)")
        .bind(record).bind(data.as_str()).bind(encode(&policy)?)
        .bind(now().to_rfc3339()).execute(connection).await?;
    Ok((changed.rows_affected() == 1).then_some(SavedPolicy {
        controller_uid: String::new(),
        revision: 1,
        policy,
    }))
}

/// Imports an Organ-owned synchronized policy and invalidates cached share cursors.
///
/// # Errors
/// Returns an error for an invalid policy or revision, or a failed write.
pub async fn import(
    pool: &SqlitePool,
    record: &str,
    data: Data,
    saved: &SavedPolicy,
) -> Result<(), StoreError> {
    saved.policy.validate_for(data).map_err(invalid)?;
    let revision = i64::try_from(saved.revision)
        .ok()
        .filter(|revision| *revision > 0 && saved.controller_uid.is_empty())
        .ok_or_else(|| invalid("Invalid synchronized visibility policy"))?;
    query("INSERT INTO data_visibility(record_uid,data,controller_uid,revision,policy_json,updated_at) VALUES(?,?,NULL,?,?,?) ON CONFLICT(record_uid,data) DO UPDATE SET revision=excluded.revision,policy_json=excluded.policy_json,updated_at=excluded.updated_at")
        .bind(record).bind(data.as_str()).bind(revision).bind(encode(&saved.policy)?)
        .bind(now().to_rfc3339()).execute(pool).await?;
    query("UPDATE organ_contact SET share_seen_seq=NULL WHERE share_protein IS NOT NULL")
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use nucleus::RecordKind;
    use serde_json::json;
    use sqlx::{SqlitePool, query};

    use super::{
        Condition, Data, MAX_POLICY_BYTES, Policy, decisions_for_person_on, denied_for_person_on,
        save_on,
    };
    use crate::{
        Store,
        exact::zero,
        records::{NewRecord, create},
        write_tx,
    };

    async fn record(pool: &SqlitePool, kind: RecordKind, head: &str) -> String {
        create(
            pool,
            NewRecord {
                slug: None,
                kind,
                head,
                body: "",
                quantity: zero(),
            },
        )
        .await
        .unwrap()
        .uid
    }

    #[tokio::test]
    async fn person_audiences_reject_controller_conflicts_and_oversized_policies() {
        let store = Store::open_memory().await.unwrap();
        let pool = &store.pool;
        let target = record(pool, RecordKind::Plain, "Protected").await;
        let controller = record(pool, RecordKind::Person, "Controller").await;
        let policy = Policy {
            include: vec![Condition {
                persons: vec![controller.clone()],
                ..Default::default()
            }],
            exclude: vec![],
        };
        let mut tx = write_tx(pool).await.unwrap();
        assert_eq!(
            save_on(
                &mut tx,
                &target,
                Data::Record,
                Some(&controller),
                &policy,
                0
            )
            .await
            .unwrap(),
            1
        );
        let stale = save_on(&mut tx, &target, Data::Record, None, &policy, 1)
            .await
            .unwrap_err();
        assert!(stale.to_string().contains("belongs to another controller"));
        let exhausted = save_on(
            &mut tx,
            &target,
            Data::Record,
            Some(&controller),
            &policy,
            u64::MAX,
        )
        .await
        .unwrap_err();
        assert!(
            exhausted
                .to_string()
                .contains("Visibility revision limit reached")
        );
        assert!(
            !denied_for_person_on(&mut tx, &controller)
                .await
                .unwrap()
                .contains(&target)
        );

        query(
            "UPDATE data_visibility SET policy_json = ? WHERE record_uid = ? AND data = 'record'",
        )
        .bind(json!({"padding": "x".repeat(MAX_POLICY_BYTES)}).to_string())
        .bind(&target)
        .execute(&mut *tx)
        .await
        .unwrap();
        let denied = denied_for_person_on(&mut tx, &controller)
            .await
            .unwrap_err();
        assert!(
            denied
                .to_string()
                .contains("Record audience exceeds its byte limit")
        );
        let decisions = decisions_for_person_on(&mut tx, &controller, Data::Record)
            .await
            .unwrap_err();
        assert!(
            decisions
                .to_string()
                .contains("Audience exceeds its byte limit")
        );
    }
}
