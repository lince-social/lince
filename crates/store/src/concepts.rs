use chrono::Utc;
use sqlx::{Row, SqlitePool};
use std::collections::{HashSet, VecDeque};

use crate::StoreError;

pub async fn create(
    pool: &SqlitePool,
    canonical_name: &str,
    parents: &[&str],
) -> Result<String, StoreError> {
    let lingua_uid = crate::linguas::ensure_local(pool).await?;
    create_in(pool, &lingua_uid, canonical_name, parents).await
}

pub async fn create_in(
    pool: &SqlitePool,
    lingua_uid: &str,
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
    crate::linguas::adopt(pool, lingua_uid, &uid).await?;
    crate::sync_ops::log_local(
        pool,
        "concept",
        &uid,
        "canonical_name",
        crate::sync_ops::OpKind::Set,
        Some(serde_json::json!(canonical_name).to_string()),
    )
    .await?;
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

pub async fn rename(
    pool: &SqlitePool,
    concept_uid: &str,
    canonical_name: &str,
) -> Result<bool, StoreError> {
    let renamed = sqlx::query("UPDATE concept SET canonical_name = ? WHERE uid = ?")
        .bind(canonical_name)
        .bind(concept_uid)
        .execute(pool)
        .await?
        .rows_affected()
        > 0;
    if renamed {
        crate::sync_ops::log_local(
            pool,
            "concept",
            concept_uid,
            "canonical_name",
            crate::sync_ops::OpKind::Set,
            Some(serde_json::json!(canonical_name).to_string()),
        )
        .await?;
    }
    Ok(renamed)
}

pub async fn add_parent(
    pool: &SqlitePool,
    concept_uid: &str,
    parent_uid: &str,
) -> Result<(), StoreError> {
    if concept_uid == parent_uid
        || descendants_including(pool, concept_uid)
            .await?
            .contains(&parent_uid.to_string())
    {
        return Err(sqlx::Error::Protocol(
            "concept hierarchy must remain acyclic".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO concept_parent (concept_uid, parent_uid) VALUES (?, ?)
         ON CONFLICT(concept_uid, parent_uid) DO NOTHING",
    )
    .bind(concept_uid)
    .bind(parent_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_parent(
    pool: &SqlitePool,
    concept_uid: &str,
    parent_uid: &str,
) -> Result<bool, StoreError> {
    Ok(
        sqlx::query("DELETE FROM concept_parent WHERE concept_uid = ? AND parent_uid = ?")
            .bind(concept_uid)
            .bind(parent_uid)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

pub async fn delete(pool: &SqlitePool, concept_uid: &str) -> Result<bool, StoreError> {
    let mut transaction = crate::write_tx(pool).await?;
    sqlx::query("DELETE FROM concept_name WHERE concept_uid = ?")
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM concept_parent WHERE concept_uid = ? OR parent_uid = ?")
        .bind(concept_uid)
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM concept_equivalence WHERE a_uid = ? OR b_uid = ?")
        .bind(concept_uid)
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM lingua_concept WHERE concept_uid = ?")
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?;
    let deleted = sqlx::query("DELETE FROM concept WHERE uid = ?")
        .bind(concept_uid)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        > 0;
    if deleted {
        crate::sync_ops::log_local_tx(
            &mut transaction,
            "concept",
            concept_uid,
            "",
            crate::sync_ops::OpKind::Tombstone,
            None,
        )
        .await?;
    }
    transaction.commit().await?;
    Ok(deleted)
}

pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<String>, StoreError> {
    if let Some(row) = sqlx::query("SELECT uid FROM concept WHERE canonical_name = ? OR uid = ?")
        .bind(token)
        .bind(token)
        .fetch_optional(pool)
        .await?
    {
        return Ok(Some(row.get("uid")));
    }
    let mut answering: Vec<String> = sqlx::query(
        "SELECT DISTINCT concept_uid FROM concept_name WHERE name = ? ORDER BY concept_uid",
    )
    .bind(token)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("concept_uid"))
    .collect();
    if answering.len() > 1 {
        return Err(sqlx::Error::Protocol(format!(
            "`{token}` is a name several concepts answer to ({}); say which one",
            answering.join(", ")
        )));
    }
    Ok(answering.pop())
}

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
    let lingua_uid = crate::linguas::ensure_local(pool).await?;
    crate::linguas::adopt(pool, &lingua_uid, uid).await?;
    Ok(())
}

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

#[derive(Debug, Clone)]
pub struct ConceptRow {
    pub uid: String,
    pub canonical_name: String,
    pub instinct: Option<String>,
    pub parents: Vec<String>,
}

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

pub async fn set_conversion_exact(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
    numerator: i128,
    denominator: i128,
) -> Result<(), StoreError> {
    if numerator <= 0 || denominator <= 0 {
        return Err(sqlx::Error::Protocol(format!(
            "conversion ratio must be positive, got {numerator}/{denominator}"
        )));
    }
    sqlx::query(
        "INSERT INTO concept_conversion (a_uid, b_uid, numerator, denominator)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(a_uid, b_uid) DO UPDATE
           SET numerator = excluded.numerator, denominator = excluded.denominator",
    )
    .bind(a_uid)
    .bind(b_uid)
    .bind(numerator.to_string())
    .bind(denominator.to_string())
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM concept_conversion WHERE a_uid = ? AND b_uid = ?")
        .bind(b_uid)
        .bind(a_uid)
        .execute(pool)
        .await?;
    Ok(())
}

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
    let exact = crate::exact::from_f64(factor);
    let denominator = 10_i128
        .checked_pow(u32::from(exact.scale()))
        .ok_or_else(|| sqlx::Error::Protocol("conversion factor scale overflows".into()))?;
    set_conversion_exact(pool, a_uid, b_uid, exact.mantissa(), denominator).await
}

pub async fn convert(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
    quantity: f64,
) -> Result<Option<f64>, StoreError> {
    if from_uid == to_uid {
        return Ok(Some(quantity));
    }
    let Some((numerator, denominator)) = conversion_ratio(pool, from_uid, to_uid).await? else {
        return Ok(None);
    };
    #[allow(clippy::cast_precision_loss)]
    Ok(Some(quantity * (numerator as f64) / (denominator as f64)))
}

pub async fn convert_exact(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
    amount: nucleus::DecimalValue,
    scale: u8,
    rounding: nucleus::karma::Rounding,
) -> Result<Option<nucleus::karma::RoundedDecimal>, StoreError> {
    if from_uid == to_uid {
        return Ok(amount
            .rescale(scale)
            .map(|value| nucleus::karma::RoundedDecimal { value, exact: true })
            .or_else(|| amount.mul_ratio(1, 1, scale, rounding)));
    }
    let Some((numerator, denominator)) = conversion_ratio(pool, from_uid, to_uid).await? else {
        return Ok(None);
    };
    amount
        .mul_ratio(numerator, denominator, scale, rounding)
        .map(Some)
        .ok_or_else(|| {
            sqlx::Error::Protocol(format!(
                "converting {from_uid} -> {to_uid} overflows the exact range"
            ))
        })
}

async fn conversion_ratio(
    pool: &SqlitePool,
    from_uid: &str,
    to_uid: &str,
) -> Result<Option<(i128, i128)>, StoreError> {
    let direct = read_ratio(pool, from_uid, to_uid).await?;
    let ratio = match direct {
        Some((numerator, denominator)) => (numerator, denominator),
        None => match read_ratio(pool, to_uid, from_uid).await? {
            Some((numerator, denominator)) => (denominator, numerator),
            None => return Ok(None),
        },
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
    Ok(Some(ratio))
}

async fn read_ratio(
    pool: &SqlitePool,
    a_uid: &str,
    b_uid: &str,
) -> Result<Option<(i128, i128)>, StoreError> {
    let Some(row) = sqlx::query(
        "SELECT numerator, denominator FROM concept_conversion WHERE a_uid = ? AND b_uid = ?",
    )
    .bind(a_uid)
    .bind(b_uid)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let numerator: String = row.get("numerator");
    let denominator: String = row.get("denominator");
    let parse = |value: &str| -> Result<i128, StoreError> {
        value.parse::<i128>().map_err(|_| {
            sqlx::Error::Protocol(format!("conversion ratio {value:?} is not an integer"))
        })
    };
    Ok(Some((parse(&numerator)?, parse(&denominator)?)))
}
