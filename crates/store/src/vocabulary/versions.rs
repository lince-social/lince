//! Immutable vocabulary definitions and paginated record content versions.

use std::collections::HashMap;

use nucleus::{
    execution::now,
    vocabulary::{DefinitionVersion, Dictionary},
};
use serde_json::{Map, Value, from_str};
use sqlx::{Row, SqliteConnection, SqlitePool, query, query_as, query_scalar, sqlite::SqliteRow};

use super::invalid;
use crate::StoreError;

const MAX_DEFINITION_VERSIONS: usize = 256;
const MAX_DEFINITION_BYTES: usize = 16_384;

fn definition_row(row: SqliteRow) -> Result<DefinitionVersion, StoreError> {
    Ok(DefinitionVersion {
        concept_uid: row.try_get("concept_uid")?,
        revision: row.try_get("revision")?,
        definition: row.try_get("definition")?,
        created_at: row.try_get("created_at")?,
    })
}

/// Reads a concept's saved definitions in revision order.
///
/// # Errors
/// Returns an error if the definitions cannot be queried or decoded.
pub async fn definitions_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<Vec<DefinitionVersion>, StoreError> {
    query("SELECT * FROM concept_definition_version WHERE concept_uid=? ORDER BY revision")
        .bind(concept)
        .try_map(definition_row)
        .fetch_all(connection)
        .await
}

/// Groups saved definitions by concept, keeping each history in revision order.
///
/// # Errors
/// Returns an error if the definitions cannot be queried or decoded.
pub async fn definition_versions(
    pool: &SqlitePool,
) -> Result<HashMap<String, Vec<DefinitionVersion>>, StoreError> {
    let mut versions = HashMap::<String, Vec<DefinitionVersion>>::new();
    let rows = query("SELECT * FROM concept_definition_version ORDER BY concept_uid,revision")
        .try_map(definition_row)
        .fetch_all(pool)
        .await?;
    for version in rows {
        if let Some(history) = versions.get_mut(&version.concept_uid) {
            history.push(version);
        } else {
            versions.insert(version.concept_uid.clone(), vec![version]);
        }
    }
    Ok(versions)
}

/// Appends a changed definition only when its current revision matches the caller's expectation.
///
/// # Errors
/// Returns an error for invalid text, a missing concept, a stale or exhausted revision, or failed storage.
pub async fn revise_definition_on(
    connection: &mut SqliteConnection,
    concept: &str,
    expected_revision: i64,
    definition: &str,
) -> Result<i64, StoreError> {
    let definition = definition.trim();
    if definition.is_empty() || definition.len() > MAX_DEFINITION_BYTES {
        return Err(invalid("Describe the meaning in 1–16384 bytes"));
    }
    let current: Option<(i64,String)> = query_as("SELECT revision,definition FROM concept_definition_version WHERE concept_uid=? ORDER BY revision DESC LIMIT 1")
        .bind(concept).fetch_optional(&mut *connection).await?;
    let (revision, previous) = current.ok_or_else(|| invalid("Choose a saved source Concept"))?;
    if expected_revision != revision {
        return Err(invalid("The definition changed; refresh before saving"));
    }
    if previous == definition {
        return Ok(revision);
    }
    if revision
        >= i64::try_from(MAX_DEFINITION_VERSIONS).map_err(|error| invalid(&error.to_string()))?
    {
        return Err(invalid(
            "This Concept has 256 definition versions; create a distinct meaning",
        ));
    }
    let next = revision + 1;
    query("INSERT INTO concept_definition_version VALUES(?,?,?,?)")
        .bind(concept)
        .bind(next)
        .bind(definition)
        .bind(now().to_rfc3339())
        .execute(&mut *connection)
        .await?;
    query("UPDATE lingua SET revision=revision+1 WHERE uid IN (SELECT lingua_uid FROM lingua_concept WHERE concept_uid=?)")
        .bind(concept).execute(connection).await?;
    Ok(next)
}

pub(super) fn validate_definitions(dictionary: &Dictionary) -> Result<(), StoreError> {
    if dictionary.definitions.len() > MAX_DEFINITION_VERSIONS {
        return Err(invalid("Too many definition versions"));
    }
    for (revision, version) in (1_i64..).zip(&dictionary.definitions) {
        if version.concept_uid != dictionary.source.concept_uid
            || version.revision != revision
            || version.definition.len() > MAX_DEFINITION_BYTES
            || version.created_at.is_empty()
            || version.created_at.len() > 64
            || revision == 1 && version.definition != dictionary.source.definition
        {
            return Err(invalid("Invalid saved definition history"));
        }
    }
    Ok(())
}

pub(super) async fn save_definitions_on(
    connection: &mut SqliteConnection,
    dictionary: &Dictionary,
) -> Result<(), StoreError> {
    for version in &dictionary.definitions {
        let previous: Option<String> = query_scalar(
            "SELECT definition FROM concept_definition_version WHERE concept_uid=? AND revision=?",
        )
        .bind(&version.concept_uid)
        .bind(version.revision)
        .fetch_optional(&mut *connection)
        .await?;
        if let Some(previous) = previous {
            if previous != version.definition {
                return Err(invalid(
                    "An incoming definition conflicts with a saved version",
                ));
            }
        } else {
            query("INSERT INTO concept_definition_version VALUES(?,?,?,?)")
                .bind(&version.concept_uid)
                .bind(version.revision)
                .bind(&version.definition)
                .bind(&version.created_at)
                .execute(&mut *connection)
                .await?;
        }
    }
    Ok(())
}

/// Reads one newest-first page of saved record content and its total count.
///
/// # Errors
/// Returns an error if the count, page, or stored content cannot be read or decoded.
pub async fn record_history_on(
    connection: &mut SqliteConnection,
    record: &str,
    offset: u32,
) -> Result<Value, StoreError> {
    let count: i64 = query_scalar("SELECT COUNT(*) FROM record_content_version WHERE record_uid=?")
        .bind(record)
        .fetch_one(&mut *connection)
        .await?;
    let versions = query("SELECT revision,content,saved_at FROM record_content_version WHERE record_uid=? ORDER BY revision DESC LIMIT 30 OFFSET ?")
        .bind(record).bind(offset).try_map(|row| {
            let content = from_str(row.try_get("content")?)
                .map_err(|error| invalid(&error.to_string()))?;
            Ok(Value::Object(Map::from_iter([
                ("revision".into(), row.try_get::<i64, _>("revision")?.into()),
                ("saved_at".into(), row.try_get::<String, _>("saved_at")?.into()),
                ("content".into(), content),
            ])))
        }).fetch_all(connection).await?;
    Ok(Value::Object(Map::from_iter([
        ("record".into(), record.into()),
        ("count".into(), count.into()),
        ("offset".into(), offset.into()),
        ("versions".into(), Value::Array(versions)),
    ])))
}
