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

pub async fn ensure(pool: &SqlitePool, canonical_name: &str) -> Result<String, StoreError> {
    if let Some(uid) = resolve(pool, canonical_name).await? {
        return Ok(uid);
    }
    create(pool, canonical_name, &[]).await
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
    Ok(
        sqlx::query("SELECT concept_uid FROM concept_name WHERE name = ? LIMIT 1")
            .bind(token)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get("concept_uid")),
    )
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

/// Adopt a foreign concept preserving its uid and lineage (blueprint III.2):
/// importing = inserting concept rows; re-adoption is a no-op. Parent edges
/// are added for parents that exist locally (they usually ride the same
/// package).
pub async fn adopt(
    pool: &SqlitePool,
    uid: &str,
    canonical_name: &str,
    origin_organ: Option<&str>,
    parents: &[String],
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO concept (uid, canonical_name, origin_organ, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(uid)
    .bind(canonical_name)
    .bind(origin_organ)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    for parent in parents {
        sqlx::query("INSERT OR IGNORE INTO concept_parent (concept_uid, parent_uid) VALUES (?, ?)")
            .bind(uid)
            .bind(parent)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Declare two concepts the same thing across dialects (blueprint III.1):
/// `concept_equivalence` lets Senses match `@apple` against a stranger's
/// `@maçã-fuji` without a central authority.
pub async fn declare_equivalence(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
    declared_by: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO concept_equivalence (a_uid, b_uid, declared_by) VALUES (?, ?, ?)",
    )
    .bind(a_uid)
    .bind(b_uid)
    .bind(declared_by)
    .execute(pool)
    .await?;
    Ok(())
}

/// One Lingua concept row for the `source: concept` Protein.
#[derive(Debug, Clone)]
pub struct ConceptRow {
    pub uid: String,
    pub canonical_name: String,
    pub instinct: Option<String>,
    pub parents: Vec<String>,
}

/// Every concept with its direct parents — the Lingua surface.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<ConceptRow>, StoreError> {
    let mut parents: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for row in sqlx::query("SELECT concept_uid, parent_uid FROM concept_parent")
        .fetch_all(pool)
        .await?
    {
        parents
            .entry(row.get("concept_uid"))
            .or_default()
            .push(row.get("parent_uid"));
    }
    Ok(
        sqlx::query("SELECT uid, canonical_name, instinct FROM concept ORDER BY canonical_name")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| {
                let uid: String = r.get("uid");
                ConceptRow {
                    parents: parents.remove(&uid).unwrap_or_default(),
                    uid,
                    canonical_name: r.get("canonical_name"),
                    instinct: r.get("instinct"),
                }
            })
            .collect(),
    )
}

/// A concept's canonical (snake_case) name, when the uid exists.
pub async fn canonical_name(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT canonical_name FROM concept WHERE uid = ?")
            .bind(concept_uid)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get("canonical_name")),
    )
}

/// The concept plus every ancestor in the parent DAG, breadth-first — nearest
/// ancestors come earlier in the result.
pub async fn ancestors_including(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Vec<String>, StoreError> {
    let mut seen: HashSet<String> = HashSet::from([concept_uid.to_string()]);
    let mut queue: VecDeque<String> = VecDeque::from([concept_uid.to_string()]);
    let mut out = vec![concept_uid.to_string()];
    while let Some(current) = queue.pop_front() {
        let parents = sqlx::query("SELECT parent_uid FROM concept_parent WHERE concept_uid = ?")
            .bind(&current)
            .fetch_all(pool)
            .await?;
        for parent in parents {
            let uid: String = parent.get("parent_uid");
            if seen.insert(uid.clone()) {
                out.push(uid.clone());
                queue.push_back(uid);
            }
        }
    }
    Ok(out)
}

/// Fallback semantics (blueprint III.1): an engine that doesn't know
/// `@blocks-softly` treats it as its parent `@blocks`. Returns `concept_uid`
/// itself when it is already known, else the nearest ancestor present in
/// `known` (breadth-first up the parent DAG), else `None`.
pub async fn nearest_ancestor_in(
    pool: &SqlitePool,
    concept_uid: &str,
    known: &[String],
) -> Result<Option<String>, StoreError> {
    for candidate in ancestors_including(pool, concept_uid).await? {
        if known.contains(&candidate) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

/// Unit conversion (blueprint III.1): declare `1 a = factor b`. One
/// authoritative row per unordered pair — the inverse direction is derived at
/// read time (`b -> a` is `1/factor`), so an existing reverse row is removed
/// on upsert to keep the pair consistent. Factors must be finite and positive.
pub async fn set_conversion(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
    factor: f64,
) -> Result<(), StoreError> {
    if !factor.is_finite() || factor <= 0.0 {
        return Err(sqlx::Error::Protocol(format!(
            "conversion factor must be finite and positive, got {factor}"
        )));
    }
    sqlx::query(
        "INSERT INTO concept_conversion (a_uid, b_uid, factor) VALUES (?, ?, ?)
         ON CONFLICT(a_uid, b_uid) DO UPDATE SET factor = excluded.factor",
    )
    .bind(a_uid)
    .bind(b_uid)
    .bind(factor)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM concept_conversion WHERE a_uid = ? AND b_uid = ?")
        .bind(b_uid)
        .bind(a_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Convert `quantity` from one unit concept to another. `None` unless a
/// declared conversion (direct or derived inverse) exists AND the two concepts
/// share a dimension — at least one common ancestor in the parent DAG
/// (blueprint III.1: "only within a shared parent dimension"). Same-uid
/// conversion is the identity.
pub async fn convert(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
    quantity: f64,
) -> Result<Option<f64>, StoreError> {
    if from_uid == to_uid {
        return Ok(Some(quantity));
    }

    let direct = sqlx::query("SELECT factor FROM concept_conversion WHERE a_uid = ? AND b_uid = ?")
        .bind(from_uid)
        .bind(to_uid)
        .fetch_optional(pool)
        .await?
        .map(|row| row.get::<f64, _>("factor"));
    let factor = match direct {
        Some(factor) => factor,
        None => {
            let inverse =
                sqlx::query("SELECT factor FROM concept_conversion WHERE a_uid = ? AND b_uid = ?")
                    .bind(to_uid)
                    .bind(from_uid)
                    .fetch_optional(pool)
                    .await?
                    .map(|row| row.get::<f64, _>("factor"));
            match inverse {
                Some(factor) => 1.0 / factor,
                None => return Ok(None),
            }
        }
    };

    let from_dimension: HashSet<String> = ancestors_including(pool, from_uid)
        .await?
        .into_iter()
        .collect();
    let shares_dimension = ancestors_including(pool, to_uid)
        .await?
        .iter()
        .any(|ancestor| from_dimension.contains(ancestor));
    if !shares_dimension {
        return Ok(None);
    }

    Ok(Some(quantity * factor))
}
