//! Lingua repository (blueprint Part III): concepts with multilingual names,
//! a parent DAG, and equivalences. One vocabulary, four jobs — record concepts,
//! units, link kinds, and the Instinct tier all live here.

use chrono::Utc;
use sqlx::{Row, SqlitePool};
use std::collections::{HashSet, VecDeque};

use crate::StoreError;

pub async fn create(
    pool: &SqlitePool,
    canonical_name: &str,
    parents: &[&str],
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("c");
    sqlx::query("INSERT INTO concept (uid, canonical_name, created_at) VALUES (?, ?, ?)")
        .bind(&uid)
        .bind(canonical_name)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    for parent in parents {
        sqlx::query("INSERT INTO concept_parent (concept_uid, parent_uid) VALUES (?, ?)")
            .bind(&uid)
            .bind(parent)
            .execute(pool)
            .await?;
    }
    Ok(uid)
}

pub async fn add_name(
    pool: &SqlitePool,
    concept_uid: &str,
    lang: &str,
    name: &str,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO concept_name (concept_uid, lang, name) VALUES (?, ?, ?)")
        .bind(concept_uid)
        .bind(lang)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}

/// Resolve `@name`: canonical first, then any language name, then uid.
pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<String>, StoreError> {
    if let Some(row) = sqlx::query("SELECT uid FROM concept WHERE canonical_name = ? OR uid = ?")
        .bind(token)
        .bind(token)
        .fetch_optional(pool)
        .await?
    {
        return Ok(Some(row.get("uid")));
    }
    Ok(sqlx::query("SELECT concept_uid FROM concept_name WHERE name = ? LIMIT 1")
        .bind(token)
        .fetch_optional(pool)
        .await?
        .map(|r| r.get("concept_uid")))
}

/// The root plus every descendant in the parent DAG — powers `concept_in @food`
/// matching `@apple` via `apple -> fruit -> food` (blueprint III.1).
pub async fn descendants_including(
    pool: &SqlitePool,
    root_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let mut seen: HashSet<String> = HashSet::from([root_uid.to_string()]);
    let mut queue: VecDeque<String> = VecDeque::from([root_uid.to_string()]);
    let mut out = vec![root_uid.to_string()];
    while let Some(current) = queue.pop_front() {
        let children = sqlx::query("SELECT concept_uid FROM concept_parent WHERE parent_uid = ?")
            .bind(&current)
            .fetch_all(pool)
            .await?;
        for child in children {
            let uid: String = child.get("concept_uid");
            if seen.insert(uid.clone()) {
                out.push(uid.clone());
                queue.push_back(uid);
            }
        }
    }
    Ok(out)
}
